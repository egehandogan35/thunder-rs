# Thunder-rs

⚠️ **NOTICE**: This library is currently under development and not yet published to crates.io. The WebSocket implementation has been tested with the Autobahn test suite, but the HTTP components are still undergoing testing.

A high-performance HTTP and WebSocket server implementation in Rust, featuring an event-driven architecture and room-based WebSocket functionality.

## Autobahn Testing

You can run the Autobahn test suite yourself to verify the WebSocket implementation:

### Quick Start

```bash
# Build the test container
docker build -t autobahn-tester-custom .

# Run tests and generate reports
docker run -v "$pwd/reports:/app/reports" autobahn-tester-custom
```

### Test Details

- Test results are available in `reports/index.json`
- WebSocket compression (per-message deflate) is not supported yet but planned for future releases
- Use `helperScript.js` to analyze results:
  ```bash
  node helperScript.js
  ```

For complete test configuration and setup, see the [**Docker**](docker/) folder.

## Status

- **WebSocket Server**: ✅ Tested with Autobahn Test Suite
- **HTTP Server**: 🚧 Under Development & Testing
- **Crates.io**: 📦 Not yet published

## Features

### WebSocket Server (Autobahn Tested)

- Event-driven architecture with connection callbacks
- Room-based messaging system for group communication
- Support for all WebSocket frame types (Text, Binary, Ping/Pong, Close)
- Automatic ping/pong handling for connection health
- Message fragmentation support for large payloads
- Reconnection with exponential backoff
- JSON message serialization support
- RFC 6455 compliant error handling

### HTTP Server (Under Development)

Built on top of Hyper, providing:

- Middleware chain support
- Static file serving with validation
- Custom routing with method-based handlers
- CORS support with flexible configuration
- Content type handling
- Error handling with custom error types

## Quick Start

Here's an example of running both HTTP and WebSocket servers on the same port:

```rust
use thunder_rs::server::Server;
use thunder_rs::ws::{Opcode, ReadResult};
use thunder_rs::http::{HeaderMap, HeaderValue, HttpMethod, ContentHeader};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Initialize server
    let mut server = Server::new("127.0.0.1:9001");

    // Configure HTTP routes
    let http_server = server.http_server();
    let handler = |mut req: Req, mut headers: HeaderMap| async move {
        // Set cookies example
        headers.insert(
            "Set-Cookie",
            HeaderValue::from_static("cookie1=cookie-value"),
        );
        headers.append(
            "Set-Cookie",
            HeaderValue::from_static("cookie2=cookie-value2"),
        );

        // Handle request data
        let result = match req.get_data::<String>().await {
            Ok(Some(data)) => Ok(data),
            Ok(None) => Ok("No data".to_string()),
            Err(_) => Ok("Error".to_string()),
        };
        (result, headers)
    };

    http_server.router(
        HttpMethod::POST,
        "/test",
        handler,
        None::<MiddlewareArc>,
        Some(ContentHeader::TextPlain),
    );

    // Configure WebSocket handlers
    let websocket_server = server.websocket_server();
    websocket_server.on_connect(|socket: Arc<Socket>| {
        tokio::spawn(async move {
            while let Ok(Some(message)) = socket.read().await {
                match message {
                    ReadResult::Text(text) => {
                        socket.send(Opcode::Text, &text).await.unwrap();
                    }
                    ReadResult::Binary(payload) => {
                        socket.send(Opcode::Binary, &payload).await.unwrap();
                    }
                    ReadResult::Close(_, _) => break,
                    _ => continue,
                }
            }
        });
    });

    // Start server
    let server = Arc::new(server);
    server.start().await;
}
```

This example demonstrates:

- HTTP POST endpoint with cookie handling and text response
- Echo WebSocket server
- Both servers sharing port 9001

## Advanced HTTP Features

### Middleware Chain Data Flow

The HTTP server supports a powerful middleware chain system that allows data transformation between middlewares:

```rust
// Example of middleware chain with data type transformation
let middleware1 = HttpServer::create_middleware(|mut req: Req, next: NextWrapper| {
    Box::pin(async move {
        // Get data as String, transform to JSON Value
        if let Ok(Some(data)) = req.get_data::<String>().await {
            if let Ok(num) = data.parse::<i32>() {
                req.set_data(serde_json::json!({ "number": num }));
            }
        }
        next.call(req, &next).await
    })
});

let middleware2 = HttpServer::create_middleware(|mut req: Req, next: NextWrapper| {
    Box::pin(async move {
        // Get data as JSON Value, transform back to String
        if let Ok(Some(value)) = req.get_data::<serde_json::Value>().await {
            if let Some(num) = value.get("number") {
                req.set_data(num.to_string());
            }
        }
        next.call(req, &next).await
    })
});

// Chain middlewares together
let mut chain = MiddlewareChain::new();
chain.chain(middleware1);
chain.chain(middleware2);
```

This demonstrates:

- Type-safe data transformation between middlewares
- Sequential processing of request data
- Flexible middleware composition
- Data flow: String → JSON Value → String

### Complete HTTP Example

For a full example including:

- CORS configuration
- Static file serving with validation
- Multiple middleware chains
- Header manipulation
- Error handling

See [/examples/http_examples.rs](/examples/http_examples.rs)

## WebSocket Server Usage

### Basic Echo Server

```rust
let websocket_server = server.websocket_server();
websocket_server.on_connect(|socket: Arc<Socket>| {
    tokio::spawn(async move {
        while let Ok(Some(message)) = socket.read().await {
            match message {
                ReadResult::Text(text) => {
                    socket.send(Opcode::Text, &text).await.unwrap();
                }
                ReadResult::Binary(payload) => {
                    socket.send(Opcode::Binary, &payload).await.unwrap();
                }
                ReadResult::Close(_, _) => break,
                _ => continue,
            }
        }
    });
});
```

### Room Management

```rust
// Create a room
socket.create_room(&rooms_clone, "game_room").await.unwrap();

// Join the room
Socket::insert_ws_to_room(&socket, &rooms_clone, "game_room").await.unwrap();

// Broadcast text message to room
socket.broadcast_to_room(
    &rooms_clone,
    "game_room",
    Opcode::Text,
    text.as_bytes()
).await;

// Or broadcast JSON
let data = json!({
    "type": "message",
    "content": text
});
socket.broadcast_json_to_room(&rooms_clone, "game_room", &data).await.unwrap();
```

### Room Features

- Create/remove rooms
- Join/leave rooms
- Broadcast messages (raw or JSON)
- Check room existence
- Get room members
- Find user's room
- Room-specific error handling

### Room Management API

```rust
// Room operations
socket.create_room(&rooms, "room1").await?;
socket.remove_room(&rooms, "room1").await;

// Connection management
Socket::insert_ws_to_room(&socket, &rooms, "room1").await?;
socket.remove_ws_from_room(&rooms, "room1").await;

// Broadcasting
socket.broadcast_to_room(&rooms, "room1", Opcode::Text, message).await;
socket.broadcast_json_to_room(&rooms, "room1", &json_data).await?;

// Room queries
socket.room_exists(&rooms, "room1").await;
socket.connections_vec(&rooms, "room1", Some(&connection_id)).await;
socket.find_roomid(&rooms, &connection_id).await;
```
## Installation

Add this to your `Cargo.toml`:
```toml

[dependencies]

thunder-rs = "0.1.0" # Not yet published to crates.io
```

## Documentation

Detailed documentation will be available on [docs.rs](https://docs.rs/thunder-rs) once published.

## Contributing

Contributions are welcome! Please feel free to:
- Report bugs
- Suggest features
- Submit pull requests
- Help with documentation
- Assist with HTTP server testing

## License

This project is licensed under the MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

## Acknowledgments

- Hyper for HTTP implementation
- Tokio for async runtime
- Autobahn Test Suite for WebSocket compliance testing

## In Production

- [CineParty](https://chromewebstore.google.com/detail/cineparty/kegfmeohbfiighjhlinekdodjiibpikg?hl=tr) - Chrome extension for synchronized video watching with friends. Actively used by 1,700+ users weekly.


