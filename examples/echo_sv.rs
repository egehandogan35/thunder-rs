use std::{env, sync::Arc};

use thunder_rs::{server::server::Server, ws::{opcode::Opcode, server::WebSocketServer, socket::{read::ReadResult, socket::Socket}}};

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| String::from("9001"))
        .parse()
        .expect("PORT must be a number");
    let address = format!("127.0.0.1:{}", port);
    let mut server = Server::new(&address);
    let websocket_server: &mut WebSocketServer = server.websocket_server();
    websocket_server.on_connect(move |socket: Arc<Socket>| {
        let socket_clone = Arc::clone(&socket);
        tokio::spawn(async move {
            loop {
                let message = match socket.read().await {
                    Ok(Some(read_result)) => match read_result {
                        ReadResult::Text(text) => {
                            socket_clone.send(Opcode::Text, &text).await.unwrap();
                        }
                        ReadResult::Binary(payload) => {
                            socket_clone.send(Opcode::Binary, &payload).await.unwrap();
                        }
                        ReadResult::Ping(payload) => {}
                        ReadResult::Pong => {}
                        ReadResult::Close(status, reason) => {
                            break;
                        }
                        ReadResult::Unknown => {
                            println!("Unsupported message received");
                        }
                    },
                    Ok(None) => {
                        println!("Connection closed");
                        break;
                    }
                    Err(e) => {
                        break;
                    }
                };
            }
        });
    });

    let server = Arc::new(server);
    let _ = server.start().await;
}
