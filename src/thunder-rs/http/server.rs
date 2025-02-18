use super::error::HttpError;
use super::routes::DataType;
use super::routes::Params;
use super::routes::Req;
use super::routes::Res;
use crate::http::routes::Route;
use crate::http::Bytes;
use crate::http::Infallible;
use crate::http::Response;
use crate::http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::HeaderMap;
use std::sync::Arc;
pub struct HttpServer {
    routes: Vec<Route>,
    options: Options,
}
struct Options {
    max_size: usize,
}
impl HttpServer {
    pub fn new() -> Self {
        HttpServer {
            routes: Vec::new(),
            options: Options { max_size: 32000 },
        }
    }
    pub fn set_max_size(&mut self, max_size: usize) {
        self.options.max_size = max_size;
    }
    pub fn get_size(&self) -> usize {
        self.options.max_size
    }
    pub fn add(&mut self, route: Route) {
        self.routes.push(route);
    }
    pub fn get_routes(&self) -> &Vec<Route> {
        &self.routes
    }
    //Helper functions to create a boxbody
    pub fn create_boxbody(data: String) -> BoxBody<Bytes, Infallible> {
        BoxBody::new(http_body_util::Full::new(Bytes::from(data)))
    }
    pub fn create_boxbody_bytes(data: Bytes) -> BoxBody<Bytes, HttpError> {
        BoxBody::new(http_body_util::Full::new(data).map_err(|_| HttpError::BodyCreationError))
    }
}
pub async fn boxbody_to_string(
    body: BoxBody<Bytes, HttpError>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let collected_result = body
        .collect()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    let data: Vec<u8> = collected_result.to_bytes().to_vec();
    String::from_utf8(data).map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}
pub async fn boxbody_to_bytes(
    body: &mut BoxBody<Bytes, HttpError>,
) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
    let collected_result = body
        .collect()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    Ok(collected_result.to_bytes())
}
pub fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}
/// Process the request body and call the handler
/// Size is the max size of the request body
/// Size can be set by the user *Default is 32000*
pub async fn process_body<F, D>(
    req: Req,
    path: String,
    params: Params,
    handler: Arc<F>,
    data: D,
    size: usize,
) -> Res
where
    F: Fn(
        http_body_util::Collected<Bytes>,
        String,
        Params,
        D,
        hyper::HeaderMap,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res> + Send + 'static>>,
    D: DataType + Send + Sync + 'static + Clone,
{
    let headers = req.get_headers().clone();

    let length = headers.get("content-length");
    let request_size = match length {
        Some(length) => match length.to_str() {
            Ok(length_str) => match length_str.parse::<usize>() {
                Ok(parsed_size) => parsed_size,
                Err(_) => size,
            },
            Err(_) => size,
        },
        None => size,
    };

    if request_size > size {
        let error_message = serde_json::json!({
            "error": "Payload Too Large",
            "message": "The request payload exceeds the allowable limit."
        });
        let body = http_body_util::Full::new(Bytes::from(error_message.to_string())).boxed();
        let mut response = Response::new(body);
        *response.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
        return Res {
            res: Ok(response),
            headers: HeaderMap::new(),
        };
    }

    let body = http_body_util::BodyStream::new(req.req.into_body());
    let body_vec = body.collect().await;
    match body_vec {
        Ok(body_vec) => {
            let mut handler_headers = HeaderMap::new();

            if let Some(content_type) = headers.get(hyper::header::CONTENT_TYPE) {
                handler_headers.insert(hyper::header::CONTENT_TYPE, content_type.clone());
            }
            let res = handler(body_vec, path, params, data, handler_headers).await;
            res
        }
        Err(_e) => {
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Res {
                res: Err(HttpError::new("Internal Server Error")),
                headers: HeaderMap::new(),
            }
        }
    }
}
