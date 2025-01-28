use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::{self, AsyncReadExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::ws::opcode::Opcode;

use super::read::ReadResult;
pub struct Reader {
    pub reader: ReadHalf<TcpStream>,
}
impl Reader {
    #[inline]
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read_exact(buf).await
    }
    #[inline]
    pub async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf).await
    }
}
pub struct Writer {
    pub writer: WriteHalf<TcpStream>,
}
impl Writer {
    #[inline]
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.writer.write_all(buf).await
    }
    #[inline]
    pub async fn flush(&mut self) -> io::Result<()> {
        self.writer.flush().await
    }
}
pub struct Socket {
    pub id: String,
    pub peer_addr: std::net::SocketAddr,
    pub reader: Arc<Mutex<Reader>>,
    pub writer: Arc<Mutex<Writer>>,
    pub query_params: HashMap<String, Vec<String>>,
    pub command_sender: tokio::sync::mpsc::Sender<SocketCommand>,
    pub clean_status: AtomicBool,
}

pub enum SocketCommand {
    Reconnect,
}

impl Socket {
    pub fn new(
        id: Uuid,
        stream: TcpStream,
        query_params: HashMap<String, Vec<String>>,
    ) -> Arc<Self> {
        let peer_addr = stream.peer_addr().unwrap();
        let (reader_half, writer_half) = tokio::io::split(stream);
        let reader = Arc::new(Mutex::new(Reader {
            reader: reader_half,
        }));
        let writer = Arc::new(Mutex::new(Writer {
            writer: writer_half,
        }));
        let clean_status = AtomicBool::new(true);
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let socket = Socket {
            id: id.to_string(),
            peer_addr,
            reader,
            writer,
            query_params,
            command_sender: tx,
            clean_status,
        };
        let arc_socket = Arc::new(socket);
        tokio::spawn(Self::handle_commands(Arc::clone(&arc_socket), rx));
        arc_socket
    }
    /// Returns the reference to the socket's ID.
    pub fn id(&self) -> &String {
        &self.id
    }

    pub fn query_params(&self) -> &HashMap<String, Vec<String>> {
        &self.query_params
    }

    pub fn send_command(
        &self,
        command: SocketCommand,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<SocketCommand>> {
        self.command_sender.try_send(command)
    }
    /// It should return true if the socket is closed cleanly
    /// If it returns false, it means that the socket is closed uncleanly
    pub fn status(&self) -> bool {
        self.clean_status.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn find_client_ip(&self) -> std::net::IpAddr {
        self.peer_addr.ip()
    }

    async fn handle_commands(self: Arc<Self>, mut rx: tokio::sync::mpsc::Receiver<SocketCommand>) {
        while let Some(command) = rx.recv().await {
            match command {
                SocketCommand::Reconnect => {
                    if let Err(e) = self.reconnect().await {
                        eprintln!("Failed to reconnect: {:?}", e);
                    } else {
                        println!("Reconnected successfully");
                    }
                }
            }
        }
    }
    /// Reconnection with exponential backoff (1s initial, 60s max)
    async fn reconnect(&self) -> std::io::Result<()> {
        let mut delay = std::time::Duration::from_secs(1);
        let max_delay = std::time::Duration::from_secs(60);

        loop {
            match TcpStream::connect(self.peer_addr).await {
                Ok(stream) => {
                    let (reader, writer) = tokio::io::split(stream);
                    *self.reader.lock().await = Reader { reader };
                    *self.writer.lock().await = Writer { writer };
                    return Ok(());
                }
                Err(_e) if delay < max_delay => {
                    eprintln!("Reconnect failed, retrying in {:?}...", delay);
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
                Err(e) => {
                    eprintln!("Reconnect failed with error: {:?}", e);
                    return Err(e);
                }
            }
        }
    }

    async fn send_close_frame(&self, close_code: u16, payload: Option<Vec<u8>>) -> io::Result<()> {
        let opcode = Opcode::Close;
        let mut close_payload = vec![(close_code >> 8) as u8, (close_code & 0xFF) as u8];

        if let Some(additional_payload) = payload {
            if let Ok(valid_utf8_payload) = String::from_utf8(additional_payload) {
                close_payload.extend_from_slice(valid_utf8_payload.as_bytes());
            }
        }

        self.send(opcode, &close_payload)
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))
    }

    async fn wait_for_client_close_frame(&self) -> io::Result<()> {
        loop {
            match self.read_frame().await {
                Ok(ReadResult::Close(_, _)) => break,
                Ok(_) => continue,
                Err(e) if e.code == 10054 => {
                    break;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }

    async fn perform_close(
        &self,
        clean: bool,
        close_code: u16,
        payload: Option<Vec<u8>>,
        wait_for_client: bool,
    ) -> io::Result<()> {
        if let Err(_e) = self.send_close_frame(close_code, payload).await {
            return self.shutdown_stream().await;
        }

        if close_code == 1002 || close_code == 1003 || close_code == 1007 || close_code == 1009 {
            return self.shutdown_stream().await;
        }

        if wait_for_client {
            self.wait_for_client_close_frame().await?;
        }

        self.shutdown_stream().await?;
        self.clean_status.store(clean, Ordering::SeqCst);

        Ok(())
    }

    pub async fn close(
        &self,
        clean: bool,
        close_code: u16,
        payload: Option<Vec<u8>>,
    ) -> io::Result<()> {
        self.perform_close(clean, close_code, payload, false).await
    }

    pub async fn internal_close(
        &self,
        clean: bool,
        close_code: u16,
        payload: Option<Vec<u8>>,
    ) -> io::Result<()> {
        self.perform_close(clean, close_code, payload, true).await
    }

    async fn shutdown_stream(&self) -> io::Result<()> {
        let mut writer = self.writer.lock().await;
        let reader = self.reader.lock().await;
        drop(reader);
        writer.writer.shutdown().await?;
        Ok(())
    }
}
