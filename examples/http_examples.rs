use std::{env, sync::Arc};

use hyper::{header::HeaderValue, HeaderMap};
use thunder_rs::{http::{error::HttpError, httpmethod::HttpMethod, routes::{ContentHeader, CorsBuilder, MiddlewareChain, NextWrapper, Req}, server::HttpServer}, server::server::Server};

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| String::from("9001"))
        .parse()
        .expect("PORT must be a number");
    let address = format!("127.0.0.1:{}", port);
    let mut server = Server::new(&address);
    let http_server: &mut HttpServer = server.http_server();

    let cors = CorsBuilder::new()
        .allow_origin("*".to_string())
        .allow_methods("POST".to_string())
        .allow_headers("Content-Type".to_string())
        .allow_credentials(true)
        .expose_headers("Content-Type".to_string())
        .max_age("86400".to_string())
        .build();
    let handler = |mut req: Req, mut headers: HeaderMap| async move {
        headers.insert("Accept", HeaderValue::from_static("application/json"));
        headers.append("Accept", HeaderValue::from_static("text/plain"));

        headers.insert("Accept-Language", HeaderValue::from_static("en-US"));
        headers.append("Accept-Language", HeaderValue::from_static("fr-FR"));

        headers.insert(
            "Set-Cookie",
            HeaderValue::from_static("cookie1=cookie-value"),
        );
        headers.append(
            "Set-Cookie",
            HeaderValue::from_static("cookie2=cookie-value2"),
        );

        headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
        headers.append("Cache-Control", HeaderValue::from_static("must-revalidate"));

        headers.insert(
            "Access-Control-Allow-Methods",
            HeaderValue::from_static("GET, POST, PUT"),
        );

        let result = match req.get_data::<String>().await {
            Ok(Some(data)) => {
                println!("Handler received number: {:?}", data);
                Ok(data)
            }
            Ok(None) => {
                println!("No data found in handler");
                Ok("No data".to_string())
            }
            Err(e) => {
                println!("Error in handler: {:?}", e);
                Ok("Error".to_string())
            }
        };

        (result, headers)
    };

    let middleware1 = HttpServer::create_middleware(|mut req: Req, next: NextWrapper| {
        let next_clone = next.clone();
        Box::pin(async move {
            match req.get_data::<String>().await {
                Ok(Some(data)) => {
                    println!("Middleware1 received string: {:?}", data);
                    if let Ok(num) = data.parse::<i32>() {
                        let value = serde_json::json!({ "number": num });
                        req.set_data(value);
                        println!("Middleware1 converted to Value");
                    }
                }
                _ => println!("No string data in middleware1"),
            }
            next_clone.call(req, &next_clone).await
        })
    });

    let middleware2 = HttpServer::create_middleware(|mut req: Req, next: NextWrapper| {
        let next_clone = next.clone();
        Box::pin(async move {
            match req.get_data::<serde_json::Value>().await {
                Ok(Some(value)) => {
                    println!("Middleware2 received Value: {:?}", value);
                    if let Some(num) = value.get("number") {
                        req.set_data(num.to_string());
                    }
                }
                _ => println!("No Value data in middleware2"),
            }
            next_clone.call(req, &next_clone).await
        })
    });
    let validate_files = Box::new(|path: &str| {
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > 5 * 1024 * 1024 {
            return Err(HttpError::Message("File too large".to_string()));
        }

        let extension = std::path::Path::new(path)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(HttpError::Message("Invalid extension".to_string()))?;

        match extension.to_lowercase().as_str() {
            "html" | "css" | "js" | "png" | "jpg" => Ok(()),
            _ => Err(HttpError::Message("File type not allowed".to_string())),
        }
    });

    let result = http_server.serve_static(
        "/static",
        "src/examples/static_test/",
        ".",
        &Some(validate_files),
    );
    match result {
        Ok(_) => println!("Serving static files"),
        Err(e) => {
            println!("Error serving static files: {}", e);
        }
    }

    let cors_middleware = HttpServer::create_cors_middleware(cors);
    let mut middleware_chain = MiddlewareChain::new();
    middleware_chain.chain(middleware1);
    middleware_chain.chain(middleware2);
    middleware_chain.chain(cors_middleware);
    http_server.router(
        HttpMethod::POST,
        "/test",
        handler,
        Some(middleware_chain),
        Some(ContentHeader::TextPlain),
    );
    let server = Arc::new(server);
    server.start().await.unwrap();
}
