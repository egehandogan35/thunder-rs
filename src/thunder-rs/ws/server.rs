// server/ws/server.rs

use hyper::body::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::Duration;
use uuid::Uuid;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty};
use hyper::Response;
use hyper::StatusCode;
use std::io;

 
use super::socket::room::Room;
use super::socket::socket::Socket;
/// WebSocket server with event-driven architecture
/// Handles connections and events through callbacks
///
/// Example usage:
/// ```rust
/// let websocket_server: &mut WebSocketServer = server.websocket_server();
///
/// // Handle new connections
/// websocket_server.on_connect(|socket| {
///     tokio::spawn(async move {
///         // Handle incoming messages
///
///     });
/// });
///
/// // Handle disconnections
/// websocket_server.on_close(|socket| {
///     println!("Client {} disconnected", socket.id());
/// });
/// ```
///
pub struct WebSocketServer {
    address: String,
    connections: Arc<Mutex<HashMap<String, Arc<Socket>>>>,
     
    rooms: Arc<Mutex<HashMap<String, Room>>>,
    on_connect: Option<Arc<dyn Fn(Arc<Socket>) + Send + Sync>>,
    on_close: Option<Arc<dyn Fn(Arc<Socket>) + Send + Sync>>,
}

pub struct ConnectionEvents {
    pub on_connect: Option<Box<dyn Fn(Arc<Socket>) + Send + Sync>>,
    pub on_close: Option<Box<dyn Fn(Arc<Socket>) + Send + Sync>>,
}

impl WebSocketServer {
    #[must_use]
    pub fn new(address: &str) -> Self {
        WebSocketServer {
            address: address.to_string(),
            connections: Arc::new(Mutex::new(HashMap::new())),
             
            rooms: Arc::new(Mutex::new(HashMap::new())),
            on_connect: None,
            on_close: None,
        }
    }

    pub fn on_connect<F: Fn(Arc<Socket>) + 'static + Send + Sync>(&mut self, f: F) {
        self.on_connect = Some(Arc::new(f));
    }

    pub fn on_close<F: Fn(Arc<Socket>) + 'static + Send + Sync>(&mut self, f: F) {
        self.on_close = Some(Arc::new(f));
    }

     
    pub fn get_rooms(&self) -> Arc<Mutex<HashMap<String, Room>>> {
        self.rooms.clone()
    }
    pub async fn close_connection(&self, connection_id: String) -> io::Result<()> {
        let mut connections = self.connections.lock().await;
        if let Some(connection) = connections.remove(&connection_id) {
            connection.close(true, 1000, None).await?;
        }
        Ok(())
    }
    pub async fn close_connection_with_delay(
        &self,
        connection_id: String,
        delay: Duration,
    ) -> io::Result<()> {
        tokio::time::sleep(delay).await;
        self.close_connection(connection_id).await
    }

    fn empty() -> BoxBody<Bytes, hyper::Error> {
        Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed()
    }
}

pub async fn handle_ws(
    server: &WebSocketServer,
    stream: TcpStream,
    query_pairs: HashMap<String, Vec<String>>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let id = Uuid::new_v4();
    let id_to_string = id.to_string();

    let connection = Socket::new(id, stream, query_pairs);

    {
        let mut connections = server.connections.lock().await;
        connections.insert(id_to_string, connection.clone());
    }

    if let Some(on_connect) = &server.on_connect {
        on_connect(connection.clone());
    }

    let mut response = Response::new(WebSocketServer::empty());
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    Ok(response)
}
