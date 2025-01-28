use std::collections::HashMap;

use base64::{engine::general_purpose, Engine as _};
use sha1::Digest;
use sha1::Sha1;
use tokio::io;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const BUFFER_SIZE: usize = 1024;
/// The magic string used in the WebSocket handshake key. This is described in the RFC 6455 documentation.
/// https://datatracker.ietf.org/doc/html/rfc6455#section-1.3
const WEBSOCKET_KEY_MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// It might be changed to a more secure method in the future
/// But for now, it is a simple implementation of the WebSocket handshake
pub async fn perform_handshake(stream: &mut TcpStream) -> io::Result<()> {
    let mut buffer = [0; BUFFER_SIZE];

    let bytes_read = stream.read(&mut buffer).await?;
    let data_str = String::from_utf8_lossy(&buffer[..bytes_read]);
    let headers: HashMap<_, _> = data_str
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.splitn(2, ": ");
            parts
                .next()
                .and_then(|key| parts.next().map(|value| (key.to_lowercase(), value)))
        })
        .collect();

    if headers
        .get("upgrade")
        .is_some_and(|value| value.to_lowercase() == "websocket")
    {
        if let Some(key) = headers.get("sec-websocket-key") {
            let sha1 = Sha1::digest(format!("{}{}", key, WEBSOCKET_KEY_MAGIC).as_bytes());
            let accept_key = general_purpose::STANDARD.encode(sha1);
            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
                Upgrade: websocket\r\n\
                Connection: Upgrade\r\n\
                Sec-WebSocket-Accept: {}\r\n\
                \r\n",
                accept_key
            );
            stream.write_all(response.as_bytes()).await?;
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Sec-WebSocket-Key header is missing",
            ))
        }
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Upgrade header is not 'websocket'",
        ))
    }
}
