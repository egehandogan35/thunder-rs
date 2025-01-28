use super::parser::{parse_query_params, parse_request_path};
use crate::http::server::HttpServer;
use crate::ws::handshake::perform_handshake;
use crate::ws::server::{handle_ws, WebSocketServer};
use http_body_util::combinators::BoxBody;
use hyper::server;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Core server that handles both HTTP and WebSocket connections on the same port
/// Routes based on path:
/// - /ws/* -> WebSocket handler
/// - /*    -> HTTP handler
pub struct Server {
    #[cfg(feature = "http")]
    http_server: HttpServer,

    #[cfg(feature = "ws")]
    websocket_server: WebSocketServer,

    address: String,
}

impl Server {
    #[must_use]
    pub fn new(address: &str) -> Self {
        Server {
            #[cfg(feature = "http")]
            http_server: HttpServer::new(),

            #[cfg(feature = "ws")]
            websocket_server: WebSocketServer::new(address),

            address: address.to_string(),
        }
    }

    #[cfg(feature = "ws")]
    pub fn websocket_server(&mut self) -> &mut WebSocketServer {
        &mut self.websocket_server
    }

    #[cfg(feature = "http")]
    pub fn http_server(&mut self) -> &mut HttpServer {
        &mut self.http_server
    }

    pub async fn start(self: Arc<Self>) -> io::Result<()> {
        let listener = TcpListener::bind(&self.address).await?;

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let server = Arc::clone(&self);
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(server, stream).await {
                            eprintln!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => eprintln!("Error accepting connection: {e}"),
            }
        }
    }

    #[cfg(feature = "ws")]
    async fn handle_ws(server: Arc<Server>, stream: TcpStream, raw_path: String) {
        let io = TokioIo::new(stream);
        let stream = io.into_inner();
        let query_pairs = parse_query_params(&raw_path);
        if let Err(err) = handle_ws(&server.websocket_server, stream, query_pairs).await {
            eprintln!("Error serving WebSocket connection: {err:?}");
        }
    }

    #[cfg(feature = "http")]
    async fn handle_http(server: Arc<Server>, stream: TcpStream) {
        let io = TokioIo::new(stream);
        if let Err(err) = server::conn::http1::Builder::new()
            .serve_connection(
                io,
                service_fn(|req| {
                    let req = req.map(BoxBody::new);
                    let server = Arc::clone(&server);
                    async move { server.http_server.handle_http(req).await }
                }),
            )
            .await
        {
            eprintln!("Error serving HTTP connection: {:?}", err);
        }
    }

    async fn handle_connection(server: Arc<Server>, mut stream: TcpStream) -> io::Result<()> {
        let mut buffer = [0; 1024];
        let n = stream.peek(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }
        let path = parse_request_path(&buffer[..n]).unwrap_or_else(|| "/".to_string());

        #[cfg(feature = "ws")]
        if path.starts_with("/ws") {
            match perform_handshake(&mut stream).await {
                Ok(_) => {
                    Self::handle_ws(server, stream, path).await;
                }
                Err(e) => {
                    eprintln!("WebSocket handshake failed: {}", e);
                    if let Err(shutdown_err) = stream.shutdown().await {
                        eprintln!("Failed to shutdown stream: {}", shutdown_err);
                    }
                }
            }
            return Ok(());
        }

        #[cfg(feature = "http")]
        {
            Self::handle_http(server, stream).await;
        }

        Ok(())
    }
}
