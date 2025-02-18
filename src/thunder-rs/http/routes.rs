use super::error::HttpError;
use super::httpmethod::HttpMethod;
use super::server::{boxbody_to_bytes, empty, process_body, HttpServer};
use super::HandlerResult;
use http_body_util::Collected;
use http_body_util::{combinators::BoxBody, BodyExt};
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::StatusCode;
use hyper::{Error, Method};
use hyper::{HeaderMap, Request, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use base64::{engine::general_purpose, Engine as _};
use std::any::Any;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct Route {
    method: Method,
    path: String,
    handler: Arc<
        dyn Fn(Req) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res> + Send>>
            + Send
            + Sync,
    >,
    content_header: ContentHeader,
}
impl Route {
    pub fn get_path(&self) -> &str {
        &self.path
    }
    pub fn get_content_header(&self) -> ContentHeader {
        self.content_header
    }
}
/// Generic data type trait - supports String, Value, Vec<u8> with content type conversions

pub trait DataType: Serialize + Deserialize<'static> + Debug + Default {
    fn to_type(
        &self,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;
    fn from_content_type(
        data: &str,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;
    fn from_content_json(
        data: &Value,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;
    fn to_bytes(&self) -> Result<Bytes, Box<dyn std::error::Error>>;
}

impl DataType for Value {
    fn to_type(
        &self,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::ApplicationJson) | None => Ok(serde_json::to_value(self)?),
            Some(ContentHeader::TextPlain) => Ok(Value::String(self.to_string())),
            Some(ContentHeader::ApplicationXml) => {
                let xml = serde_xml_rs::to_string(self)?;
                Ok(Value::String(xml))
            }
            Some(ContentHeader::TextHtml) => {
                let html = format!("<html><body>{}</body></html>", self);
                Ok(Value::String(html))
            }
            _ => Err("Unsupported content type".into()),
        }
    }

    fn from_content_type(
        data: &str,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::ApplicationJson) | None => Ok(serde_json::from_str(data)?),
            Some(ContentHeader::ApplicationXml) => {
                let value = serde_xml_rs::from_str::<Value>(data)?;
                Ok(value)
            }
            Some(ContentHeader::TextPlain) => Ok(serde_json::Value::String(data.to_string())),
            _ => Err("Unsupported content type for deserialization".into()),
        }
    }

    fn from_content_json(
        data: &Value,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        match content_header {
            Some(ContentHeader::ApplicationJson) | None => Ok(data.clone()),
            _ => Err("Unsupported content type for deserialization".into()),
        }
    }
    fn to_bytes(&self) -> Result<Bytes, Box<dyn std::error::Error>> {
        let bytes = match self {
            Value::String(s) => Bytes::from(s.clone()),
            Value::Number(n) => Bytes::from(n.to_string()),
            Value::Bool(b) => Bytes::from(b.to_string()),
            Value::Array(a) => Bytes::from(serde_json::to_vec(&a)?),
            Value::Object(o) => Bytes::from(serde_json::to_vec(&o)?),
            Value::Null => Bytes::from("null".to_string()),
        };
        Ok(bytes)
    }
}

impl DataType for String {
    fn to_type(
        &self,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::TextPlain) | Some(ContentHeader::ApplicationJson) | None => {
                Ok(self.clone())
            }
            _ => Err("Unsupported content type for String".into()),
        }
    }

    fn from_content_type(
        data: &str,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::TextPlain) | Some(ContentHeader::ApplicationJson) | None => {
                Ok(data.to_string())
            }
            _ => Err("Unsupported content type for deserialization".into()),
        }
    }
    fn from_content_json(
        data: &Value,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        match content_header {
            Some(ContentHeader::TextPlain) | Some(ContentHeader::ApplicationJson) | None => {
                if let Some(string) = data.as_str() {
                    Ok(string.to_string())
                } else {
                    Ok(data.to_string())
                }
            }
            _ => Err("Unsupported content type for deserialization".into()),
        }
    }
    fn to_bytes(&self) -> Result<Bytes, Box<dyn std::error::Error>> {
        Ok(Bytes::from(self.clone()))
    }
}

impl DataType for Vec<u8> {
    fn to_type(
        &self,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::ApplicationOctetStream) | None => Ok(self.to_vec()),
            _ => Err("Unsupported content type for binary data".into()),
        }
    }

    fn from_content_type(
        data: &str,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        match content_header {
            Some(ContentHeader::ApplicationOctetStream) | None => {
                Ok(general_purpose::STANDARD.decode(data.as_bytes())?)
            }
            _ => Err("Unsupported content type for binary data".into()),
        }
    }

    fn from_content_json(
        data: &Value,
        content_header: Option<ContentHeader>,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized,
    {
        match content_header {
            Some(ContentHeader::ApplicationOctetStream) | None => {
                if let Some(string) = data.as_str() {
                    Ok(general_purpose::STANDARD.decode(string.as_bytes())?)
                } else {
                    Err("Expected data to be a string".into())
                }
            }
            _ => Err("Unsupported content type for binary data".into()),
        }
    }
    fn to_bytes(&self) -> Result<Bytes, Box<dyn std::error::Error>> {
        Ok(Bytes::from(self.clone()))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ContentHeader {
    TextPlain,
    ApplicationJson,
    ApplicationXml,
    ApplicationOctetStream,
    TextHtml,
    TextCss,
    ApplicationJavascript,
}

impl From<ContentHeader> for &'static str {
    fn from(header: ContentHeader) -> &'static str {
        match header {
            ContentHeader::TextPlain => "text/plain",
            ContentHeader::ApplicationJson => "application/json",
            ContentHeader::ApplicationXml => "application/xml",
            ContentHeader::ApplicationOctetStream => "application/octet-stream",
            ContentHeader::TextHtml => "text/html",
            ContentHeader::TextCss => "text/css",
            ContentHeader::ApplicationJavascript => "application/javascript",
        }
    }
}

impl ContentHeader {
    pub fn content_type(&self) -> &'static str {
        match self {
            ContentHeader::TextPlain => "text/plain",
            ContentHeader::ApplicationJson => "application/json",
            ContentHeader::ApplicationXml => "application/xml",
            ContentHeader::ApplicationOctetStream => "application/octet-stream",
            ContentHeader::TextHtml => "text/html",
            ContentHeader::TextCss => "text/css",
            ContentHeader::ApplicationJavascript => "application/javascript",
        }
    }
}

impl TryFrom<&str> for ContentHeader {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "text/plain" => Ok(ContentHeader::TextPlain),
            "application/json" => Ok(ContentHeader::ApplicationJson),
            "application/xml" => Ok(ContentHeader::ApplicationXml),
            "application/octet-stream" => Ok(ContentHeader::ApplicationOctetStream),
            "text/html" => Ok(ContentHeader::TextHtml),
            "text/css" => Ok(ContentHeader::TextCss),
            "application/javascript" => Ok(ContentHeader::ApplicationJavascript),
            _ => Err("Invalid content type"),
        }
    }
}
pub struct RouteBuilder<F, D> {
    method: Method,
    path: String,
    handler: Option<F>,
    content_header: ContentHeader,
    /// PhantomData to track generic type D without storing it - used for type checking during route building
    _marker: std::marker::PhantomData<D>,
}

impl<F, D> RouteBuilder<F, D>
where
    F: Fn(
            Req,
            D,
            hyper::HeaderMap,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res> + Send>>
        + Send
        + Sync
        + 'static,
    D: DataType + Send + Sync + 'static + Clone,
{
    fn new(method: HttpMethod, path: String, content_header: ContentHeader) -> Self {
        Self {
            method: Method::from(method),
            path,
            handler: None,
            content_header,
            _marker: std::marker::PhantomData,
        }
    }

    fn handler(self, handler: F) -> Self {
        Self {
            method: self.method,
            path: self.path,
            handler: Some(handler),
            content_header: self.content_header,
            _marker: self._marker,
        }
    }

    pub fn build(
        self,
        server: &mut HttpServer,
        middleware: Option<Arc<dyn Middleware + Send + Sync>>,
    ) {
        if let Some(handler) = self.handler {
            let middleware = middleware.map(MiddlewareArc).map(|arc| arc.extract());
            server.add_route(
                RouteBuilder {
                    method: self.method,
                    path: self.path,
                    handler: Some(handler),
                    content_header: self.content_header,
                    _marker: self._marker,
                },
                middleware,
            );
        }
    }
}

impl HttpServer {
    /// Checks if the request is valid and returns the appropriate response based on the route and method
    /// !This is not for validating the request body or checking every detail of the request,
    /// !It is for checking if the request and method is valid and if the route exists
    pub async fn handle_http(
        &self,
        req: Request<BoxBody<Bytes, hyper::Error>>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, HttpError> {
        let req = req.map(|body| {
            let mapped_body = body.map_err(HttpError::from);
            BoxBody::new(mapped_body)
        });

        let req_method = req.method().clone();
        let req_path = req.uri().path().to_string();

        if req_path.is_empty() || req_path.contains("..") {
            return Ok(
                self.create_error_response(HttpError::new("Invalid Path"), StatusCode::BAD_REQUEST)
            );
        }

        if req_path == "/" {
            let body = Self::create_boxbody("Server is live".to_string());
            let mut response = Response::new(body);
            *response.status_mut() = StatusCode::OK;
            return Ok(response);
        }
        let req_params = req.uri().query();
        let mut params = Params::new();

        if let Some(query_str) = req_params {
            query_str
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.split('=');
                    if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                        let decoded_key = urlencoding::decode(key).ok()?.to_string();
                        let decoded_value = urlencoding::decode(value).ok()?.to_string();
                        Some((decoded_key, decoded_value))
                    } else {
                        None
                    }
                })
                .for_each(|(key, value)| {
                    params
                        .query_params
                        .entry(key)
                        .or_insert_with(Vec::new)
                        .push(value);
                });
        }

        let trimmed_req_path = req_path.trim_end_matches('/').to_string();
        let mut method_not_allowed = false;
        let mut matched_route = None;

        for route in self.get_routes() {
            let route_method = route.method.clone();
            let route_path = route.path.trim_end_matches('/');
            // Right now this is just checking if the path is the same or if the path starts with /static
            // In the future this will be expanded to support more complex path matching
            let static_match = trimmed_req_path.starts_with("/static")
                && route_path.starts_with("/static")
                && (route_path == "/static" || route_path.starts_with("/static/"));

            if static_match {
                if req_method == route_method {
                    matched_route = Some(route);
                    method_not_allowed = false;
                    break;
                } else {
                    method_not_allowed = true;
                    continue;
                }
            }

            let route_segments: Vec<&str> =
                route_path.split('/').filter(|s| !s.is_empty()).collect();
            let path_segments: Vec<&str> = trimmed_req_path
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();

            if route_segments.len() == path_segments.len() {
                let mut matches = true;

                for (route_seg, path_seg) in route_segments.iter().zip(path_segments.iter()) {
                    if !route_seg.starts_with(':') && route_seg != path_seg {
                        matches = false;
                        break;
                    }
                }

                if matches {
                    if req_method == route_method {
                        matched_route = Some(route);
                        method_not_allowed = false;
                        break;
                    } else {
                        method_not_allowed = true;
                    }
                }
            }
        }
        let req_headers = req.headers().clone();

        if let Some(route) = matched_route {
            let route_segments: Vec<&str> =
                route.path.split('/').filter(|s| !s.is_empty()).collect();
            let path_segments: Vec<&str> = trimmed_req_path
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();

            for (pattern, actual) in route_segments.iter().zip(path_segments.iter()) {
                if pattern.starts_with(':') {
                    let param_name = pattern.trim_start_matches(':');
                    params
                        .path_params
                        .insert(param_name.to_string(), actual.to_string());
                }
            }
            let req_obj = Req {
                req,
                path: trimmed_req_path.to_string(),
                data: None,
                headers: req_headers,
                params,
            };

            let res = (route.handler)(req_obj).await;

            let mut response = match res.res {
                Ok(response) => response,
                Err(e) => {
                    return Ok(self.create_error_response(e, StatusCode::INTERNAL_SERVER_ERROR))
                }
            };

            let content_type = route.content_header.content_type();
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_str(content_type)?,
            );
            let cors_headers = response.headers().clone();
            // Handles header conflicts when same header exists in both CORS and custom headers
            // Example: If both CORS and custom headers set 'access-control-allow-methods',
            // combines them uniquely instead of overwriting (GET, POST + PUT, DELETE = GET, POST, PUT, DELETE)
            for (key, value) in res.headers.iter() {
                match key.as_str() {
                    k if k.starts_with("access-control-") && cors_headers.contains_key(k) => {
                        let existing_value = cors_headers
                            .get(k)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");

                        let new_value = value.to_str().unwrap_or("");
                        let combined_values: Vec<&str> = existing_value
                            .split(',')
                            .chain(new_value.split(','))
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();

                        let combined_str = combined_values.join(", ");

                        response.headers_mut().insert(
                            key.clone(),
                            HeaderValue::from_str(&combined_str).unwrap_or(value.clone()),
                        );
                    }
                    "set-cookie" | "accept" | "accept-charset" | "accept-encoding"
                    | "accept-language" | "allow" | "cache-control" | "connection"
                    | "content-encoding" | "content-language" | "if-match" | "if-none-match"
                    | "vary" | "via" | "warning" => {
                        if !response
                            .headers()
                            .get_all(key)
                            .iter()
                            .any(|existing| existing == value)
                        {
                            response.headers_mut().append(key.clone(), value.clone());
                        }
                    }
                    _ => {
                        response.headers_mut().insert(key.clone(), value.clone());
                    }
                }
            }

            return Ok(response);
        } else if method_not_allowed {
            let response = self.create_error_response(
                HttpError::new("Method Not Allowed"),
                StatusCode::METHOD_NOT_ALLOWED,
            );
            return Ok(response);
        } else {
            return Ok(
                self.create_error_response(HttpError::new("Not Found"), StatusCode::NOT_FOUND)
            );
        }
    }
    fn create_error_response(
        &self,
        error: HttpError,
        status_code: StatusCode,
    ) -> Response<BoxBody<Bytes, Infallible>> {
        let body = Self::create_boxbody(error.to_string());
        let mut response = Response::new(body);
        *response.status_mut() = status_code;
        response
    }

    pub fn url_param(url: &str) -> Vec<&str> {
        url.split('/').collect()
    }
    pub fn param_for_route<'a>(url: &'a str, route: &'a str) -> Option<Vec<&'a str>> {
        let url_param = Self::url_param(url);
        let route_param = Self::url_param(route);
        if url_param.len() != route_param.len() {
            return None;
        }
        let mut param = Vec::new();
        for (i, route) in route_param.iter().enumerate() {
            if route.starts_with(':') {
                param.push(url_param[i]);
            }
        }
        Some(param)
    }

    fn initialize_middleware<M>(middleware: Option<M>) -> MiddlewareArc
    where
        M: Middleware + Send + Sync + 'static,
    {
        match middleware {
            Some(m) => MiddlewareArc(Arc::new(m)),
            None => MiddlewareArc(Arc::new(MiddlewareClosure::new(default_middleware))),
        }
    }

    fn create_handler<F, Fut, D>(
        handler: F,
        content_header: Option<ContentHeader>,
    ) -> impl Fn(Collected<Bytes>, String, Params, D, HeaderMap) -> BoxFuture<'static, Res>
    where
        F: Fn(Req, HeaderMap) -> Fut + Send + Sync + 'static + Clone,
        Fut: Future<Output = (Result<D, HttpError>, HeaderMap)> + Send,
        D: DataType + Send + Sync + 'static + Clone,
    {
        let handler = Arc::new(handler);
        move |collected, path, params, mid_data, headers| {
            let handler_clone = Arc::clone(&handler);
            Box::pin(async move {
                let data = collected.to_bytes();
                let middleware_request = Req {
                    req: Request::new(Self::create_boxbody_bytes(data)),
                    path,
                    data: Some(Box::new(mid_data)),
                    headers: headers.clone(),
                    params,
                };
                let (result, modified_headers) = handler_clone(middleware_request, headers).await;

                Self::create_response(result, modified_headers, content_header)
            })
        }
    }

    fn create_response<D>(
        result: Result<D, HttpError>,
        headers: HeaderMap,
        content_header: Option<ContentHeader>,
    ) -> Res
    where
        D: DataType + Send + Sync + 'static,
    {
        match result {
            Ok(response_data) => match response_data.to_type(content_header) {
                Ok(response_bytes) => match response_bytes.to_bytes() {
                    Ok(bytes) => {
                        let data = http_body_util::Full::new(bytes).boxed();
                        let mut response = Response::new(data);
                        if let Some(content_header) = content_header {
                            response.headers_mut().insert(
                                hyper::header::CONTENT_TYPE,
                                hyper::header::HeaderValue::from_static(
                                    content_header.content_type(),
                                ),
                            );
                        } else {
                            response.headers_mut().insert(
                                hyper::header::CONTENT_TYPE,
                                hyper::header::HeaderValue::from_static("application/json"),
                            );
                        }
                        for (key, value) in headers.iter() {
                            match key.as_str() {
                                "set-cookie"
                                | "access-control-allow-headers"
                                | "access-control-allow-methods"
                                | "access-control-expose-headers"
                                | "accept"
                                | "accept-charset"
                                | "accept-encoding"
                                | "accept-language"
                                | "allow"
                                | "cache-control"
                                | "connection"
                                | "content-encoding"
                                | "content-language"
                                | "if-match"
                                | "if-none-match"
                                | "vary"
                                | "via"
                                | "warning" => {
                                    response.headers_mut().append(key.clone(), value.clone());
                                }
                                _ => {
                                    response.headers_mut().insert(key.clone(), value.clone());
                                }
                            }
                        }
                        Res {
                            res: Ok(response),
                            headers,
                        }
                    }
                    Err(e) => {
                        let mut response = Response::new(Self::create_boxbody(e.to_string()));
                        *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                        Res {
                            res: Ok(response),
                            headers,
                        }
                    }
                },
                Err(e) => {
                    let mut response = Response::new(Self::create_boxbody(e.to_string()));
                    *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                    Res {
                        res: Ok(response),
                        headers,
                    }
                }
            },
            Err(e) => Res {
                res: Err(e),
                headers,
            },
        }
    }

    pub fn router<F, Fut, M, D>(
        &mut self,
        method: HttpMethod,
        url: &str,
        handler: F,
        middleware: Option<M>,
        content_header: Option<ContentHeader>,
    ) where
        F: Fn(Req, HeaderMap) -> Fut + Send + Sync + 'static + Clone,
        Fut: Future<Output = (Result<D, HttpError>, HeaderMap)> + Send + 'static,
        M: Middleware + Send + Sync + 'static,
        D: DataType + Send + Sync + 'static + Clone,
    {
        let middleware = Self::initialize_middleware(middleware);
        let handler = Self::create_handler(handler, content_header);
        match method {
            HttpMethod::GET => {
                self.internal(HttpMethod::GET, url, handler, middleware, content_header)
            }
            HttpMethod::POST => {
                self.internal(HttpMethod::POST, url, handler, middleware, content_header)
            }
            HttpMethod::PUT => {
                self.internal(HttpMethod::PUT, url, handler, middleware, content_header)
            }
            HttpMethod::DELETE => {
                self.internal(HttpMethod::DELETE, url, handler, middleware, content_header)
            }
            HttpMethod::PATCH => {
                self.internal(HttpMethod::PATCH, url, handler, middleware, content_header)
            }
        }
    }

    pub fn create_response_with_status(
        res: Response<BoxBody<Bytes, Error>>,
        status: hyper::StatusCode,
    ) -> Pin<Box<dyn Future<Output = Res> + Send + 'static>> {
        let body = Self::create_boxbody(String::new());
        let mut new_res = Response::new(body);
        let mut headers = HeaderMap::new();
        for (key, value) in res.headers().iter() {
            headers.insert(key.clone(), value.clone());
        }
        *new_res.status_mut() = status;
        Box::pin(async move {
            Res {
                res: Ok(new_res),
                headers,
            }
        })
    }

    fn extract_request_data<D: DataType + Send + Sync>(
        data: Option<&Box<dyn Any + Send>>,
        content_header: Option<ContentHeader>,
    ) -> D {
        if let Some(data) = data {
            if let Some(str_content) = data.downcast_ref::<String>() {
                return D::from_content_type(str_content, content_header)
                    .unwrap_or_else(|_| D::default());
            }

            if let Some(value_content) = data.downcast_ref::<Value>() {
                return D::from_content_json(value_content, content_header)
                    .unwrap_or_else(|_| D::default());
            }
            D::default()
        } else {
            D::default()
        }
    }

    fn create_middleware_req(req: Req, path: String, data: Vec<u8>) -> Req {
        let headers = req.get_headers().clone();
        Req {
            req: req.req,
            path,
            data: Some(Box::new(data)),
            headers: headers.clone(),
            params: req.params,
        }
    }
    /// Builds a route with the given builder and middleware
    /// !This is the main function that builds the route and adds it to the server
    fn add_route<F, D>(
        &mut self,
        route_builder: RouteBuilder<F, D>,
        middleware: Option<Arc<dyn Middleware + Send + Sync>>,
    ) where
        // Data is transmitted as seperate from the request just to make it easier to handle
        F: Fn(Req, D, hyper::HeaderMap) -> BoxFuture<'static, Res> + Send + Sync + 'static,
        D: DataType + Send + Sync,
    {
        let handler = route_builder
            .handler
            .expect("Handler must be set before building the route");
        let handler = Arc::new(handler);
        let route = Route {
            method: route_builder.method,
            path: route_builder.path,
            handler: Arc::new(move |req| {
                let path = req.get_path().to_string();
                //empty data
                let data = Vec::new();

                let handler_clone = Arc::clone(&handler);

                let content_header = req
                    .get_headers()
                    .get("content-type")
                    .and_then(|header| header.to_str().ok())
                    .and_then(|header| ContentHeader::try_from(header).ok());

                let handler_future: Next = Arc::new(move |req: Req, next: &NextWrapper| {
                    let req_middleware = next.get_req_from_next(req);
                    let request_data: D =
                        Self::extract_request_data(req_middleware.data.as_ref(), content_header);

                    let response_headers = HeaderMap::new();

                    let handler_res =
                        (handler_clone)(req_middleware, request_data, response_headers);

                    Box::pin(async move { handler_res.await })
                });

                let middleware_req = Self::create_middleware_req(req, path, data);
                Self::handle_middleware(middleware.as_ref(), middleware_req, handler_future)
            }),
            content_header: route_builder.content_header,
        };

        self.add(route);
    }
    /// Internal route handler used by all HTTP methods
    fn internal<F, M, D>(
        &mut self,
        method: HttpMethod,
        url: &str,
        handler: F,
        middleware: M,
        content_header: Option<ContentHeader>,
    ) where
        F: Fn(Collected<Bytes>, String, Params, D, HeaderMap) -> BoxFuture<'static, Res>
            + Send
            + Sync
            + 'static,
        M: Middleware + Send + Sync + 'static,
        D: DataType + Send + Sync + 'static + Clone,
    {
        let handler = Arc::new(handler);
        let content_header = content_header.unwrap_or(ContentHeader::ApplicationJson);
        let size = self.get_size();

        let route_builder = RouteBuilder::new(method, url.to_string(), content_header).handler(
            move |req, data, _headers| {
                let handler_clone = Arc::clone(&handler);
                let path = req.path.clone();
                let params = req.params.clone();
                Box::pin(process_body(req, path, params, handler_clone, data, size))
            },
        );
        let middleware_arc = MiddlewareArc(Arc::new(middleware));
        let middleware_builder = MiddlewareBuilder::new(route_builder, middleware_arc);
        middleware_builder.build(self);
    }

    fn get_internal<F, M, D>(
        &mut self,
        url: &str,
        handler: F,
        middleware: M,
        content_header: Option<ContentHeader>,
    ) where
        F: Fn(Collected<Bytes>, String, Params, D, HeaderMap) -> BoxFuture<'static, Res>
            + Send
            + Sync
            + 'static,
        M: Middleware + Send + Sync + 'static,
        D: DataType + Send + Sync + 'static + Clone,
    {
        self.internal(HttpMethod::GET, url, handler, middleware, content_header);
    }
    /// GET-specific handler used primarily for static file serving
    /// Provides simplified interface compared to other HTTP methods
    pub fn get<F>(&mut self, url: &str, simple_handler: F, content_header: Option<ContentHeader>)
    where
        F: Fn(Bytes, Value, HeaderMap) -> Result<String, HttpError> + Send + Sync + 'static + Clone,
    {
        let middleware = MiddlewareClosure::new(default_middleware);
        let middleware_arc = MiddlewareArc(Arc::new(middleware));
        self.get_m(url, simple_handler, middleware_arc, content_header)
    }
    fn get_m<F, M>(
        &mut self,
        url: &str,
        simple_handler: F,
        middleware: M,
        content_header: Option<ContentHeader>,
    ) where
        F: Fn(Bytes, Value, HeaderMap) -> Result<String, HttpError> + Send + Sync + 'static + Clone,
        M: Middleware + Send + Sync + 'static,
    {
        let simple_handler = Arc::new(simple_handler);
        let handler = move |collected: Collected<Bytes>,
                            _path: String,
                            _params: Params,
                            data: Value,
                            headers: HeaderMap| {
            let simple_handler_clone = simple_handler.clone();
            let future: BoxFuture<'static, Res> = Box::pin(async move {
                let result = simple_handler_clone(collected.to_bytes(), data, headers);
                match result {
                    Ok(string) => {
                        let data = Self::create_boxbody(string);
                        let mut response = Response::new(data);
                        if let Some(content_header) = content_header {
                            response.headers_mut().insert(
                                hyper::header::CONTENT_TYPE,
                                hyper::header::HeaderValue::from_static(
                                    content_header.content_type(),
                                ),
                            );
                        }
                        Res {
                            res: Ok(response),
                            headers: HeaderMap::new(),
                        }
                    }
                    Err(error) => Res {
                        res: Err(error),
                        headers: HeaderMap::new(),
                    },
                }
            });
            future
        };
        let middleware_arc = MiddlewareArc(Arc::new(middleware));
        self.get_internal(url, handler, middleware_arc, content_header);
    }
    pub fn create_middleware<F, Fut>(func: F) -> impl Middleware + Send + Sync
    where
        F: Fn(Req, NextWrapper) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Res> + Send + 'static,
    {
        MiddlewareClosure::new(move |req, next| Box::pin(func(req, next)))
    }
    fn handle_middleware(
        middleware: Option<&Arc<dyn Middleware + Send + Sync>>,
        middleware_req: Req,
        handler_future: Next,
    ) -> Pin<Box<dyn Future<Output = Res> + Send>> {
        let handler_future_wrapper = NextWrapper::new(handler_future);
        if let Some(middleware) = middleware {
            let middleware_res = middleware.handle(middleware_req, handler_future_wrapper);
            Box::pin(async move {
                let result = middleware_res.await;
                let Res { res, headers } = result;
                Res { res, headers }
            })
        } else {
            handler_future_wrapper.call(middleware_req, &NextWrapper::default())
        }
    }
    pub fn create_cors_middleware(
        cors: Cors,
    ) -> MiddlewareClosure<
        impl Fn(Req, NextWrapper) -> Pin<Box<dyn Future<Output = Res> + Send>>
            + Send
            + Sync
            + Clone
            + 'static,
    > {
        MiddlewareClosure::new(move |req: Req, next: NextWrapper| {
            let mut cors_headers = HeaderMap::new();
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                hyper::header::HeaderValue::from_str(&cors.allow_origin).unwrap(),
            );
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_ALLOW_METHODS,
                hyper::header::HeaderValue::from_str(&cors.allow_methods).unwrap(),
            );
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_ALLOW_HEADERS,
                hyper::header::HeaderValue::from_str(&cors.allow_headers).unwrap(),
            );
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                hyper::header::HeaderValue::from_str(&cors.allow_credentials.to_string()).unwrap(),
            );
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_EXPOSE_HEADERS,
                hyper::header::HeaderValue::from_str(&cors.expose_headers).unwrap(),
            );
            cors_headers.insert(
                hyper::header::ACCESS_CONTROL_MAX_AGE,
                hyper::header::HeaderValue::from_str(&cors.max_age).unwrap(),
            );
            // Check if the HTTP method is allowed by CORS configuration
            let allowed_methods: Vec<&str> =
                cors.allow_methods.split(',').map(|s| s.trim()).collect();
            if !allowed_methods.contains(&req.req.method().as_str()) {
                let mut res = Response::new(empty());
                res.headers_mut().extend(cors_headers.clone());
                return HttpServer::create_response_with_status(
                    res,
                    hyper::StatusCode::METHOD_NOT_ALLOWED,
                );
            }
            // Validate requested headers against allowed headers in CORS config
            if let Some(requested_headers) = req
                .req
                .headers()
                .get(hyper::header::ACCESS_CONTROL_REQUEST_HEADERS)
            {
                let allowed_headers: Vec<&str> =
                    cors.allow_headers.split(',').map(|s| s.trim()).collect();
                let requested_headers: Vec<&str> = requested_headers
                    .to_str()
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim())
                    .collect();
                if !requested_headers
                    .iter()
                    .all(|h| allowed_headers.contains(h))
                {
                    let mut res = Response::new(empty());
                    res.headers_mut().extend(cors_headers.clone());
                    return HttpServer::create_response_with_status(
                        res,
                        hyper::StatusCode::FORBIDDEN,
                    );
                }
            }
            // Validate origin against CORS allow-origin setting
            if let Some(origin) = req.req.headers().get(hyper::header::ORIGIN) {
                if origin.to_str().unwrap() != cors.allow_origin && cors.allow_origin != "*" {
                    let mut res = Response::new(empty());
                    res.headers_mut().extend(cors_headers.clone());
                    return HttpServer::create_response_with_status(
                        res,
                        hyper::StatusCode::FORBIDDEN,
                    );
                }
            }
            // Handle OPTIONS request for preflight checks
            if req.req.method() == hyper::Method::OPTIONS
                && req
                    .req
                    .headers()
                    .contains_key(hyper::header::ACCESS_CONTROL_REQUEST_METHOD)
            {
                let mut res = Response::new(empty());
                res.headers_mut().extend(cors_headers.clone());
                return HttpServer::create_response_with_status(res, hyper::StatusCode::OK);
            }
            // Process actual request with CORS headers
            let next_clone = next.clone();
            let res_future = next_clone.call(
                Req {
                    req: req.req,
                    path: req.path,
                    data: req.data,
                    headers: req.headers,
                    params: req.params,
                },
                &next,
            );
            Box::pin(async move {
                let mut res = res_future.await;
                res.set_headers(cors_headers);
                res
            })
        })
    }
}
fn default_middleware(
    req: Req,
    next: NextWrapper,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res> + Send>> {
    next.call(req, &NextWrapper::default())
}
pub type Next = Arc<dyn Fn(Req, &NextWrapper) -> BoxFuture<'static, Res> + Send + Sync>;

#[derive(Clone)]
pub struct NextWrapper {
    func: Next,
}

impl NextWrapper {
    pub fn new(func: Next) -> Self {
        NextWrapper { func }
    }

    pub fn call(
        &self,
        req: Req,
        next_wrapper: &NextWrapper,
    ) -> Pin<Box<dyn Future<Output = Res> + Send>> {
        (self.func)(req, next_wrapper)
    }

    fn get_req_from_next(&self, req: Req) -> Req {
        Req {
            req: req.req,
            path: req.path,
            data: req.data,
            headers: req.headers,
            params: req.params,
        }
    }

    pub fn get_func(&self) -> &Next {
        &self.func
    }
    /// Recursively processes middleware chain before executing final handler
    /// Flow: middleware1 (receives client request) -> middleware2 -> handler
    /// If middleware1 modifies request, it's passed to middleware2(modified request), and so on, until the handler is called
    /// This ensures that each middleware can process and modify the request as needed before passing it to the next middleware or the final handler
    pub fn call_last(
        &self,
        req: Req,
        middlewares: Arc<[Arc<dyn Middleware + Send + Sync>]>,
        index: usize,
        handler: Next,
    ) -> Pin<Box<dyn Future<Output = Res> + Send>> {
        if index < middlewares.len() {
            let middleware = middlewares[index].clone();
            let next_wrapper = NextWrapper::new(Arc::new(move |req, next| {
                next.call_last(req, middlewares.clone(), index + 1, handler.clone())
            }));
            middleware.handle(req, next_wrapper)
        } else {
            self.call(req, &NextWrapper { func: handler })
        }
    }
}

impl Default for NextWrapper {
    fn default() -> Self {
        NextWrapper {
            func: Arc::new(move |_: Req, _: &NextWrapper| {
                Box::pin(async {
                    Res {
                        res: Ok(Response::new(BoxBody::new(http_body_util::Empty::new()))),
                        headers: HeaderMap::new(),
                    }
                })
            }),
        }
    }
}
#[derive(Default, Debug)]
pub struct Req {
    pub(crate) req: Request<BoxBody<Bytes, HttpError>>,
    path: String,
    data: Option<Box<dyn std::any::Any + Send>>,
    headers: HeaderMap,
    params: Params,
}

impl Req {
    pub fn debug_body(&self) {
        println!("{:?}", self.req.body());
    }
    pub fn get_headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    /// Gets data from the request body or middleware data
    /// Data can be different types through the chain
    /// Example: middleware1 receives a string, converts it to a number, and passes it to middleware2
    /// middleware2 receives the number, converts it to a JSON object, and passes it to the handler
    /// The handler receives the JSON object and can use it as needed
    pub async fn get_data<T: std::any::Any + Send + Clone + DeserializeOwned + Debug>(
        &mut self,
    ) -> Result<Option<T>, Box<dyn std::error::Error + Send + Sync>> {
        let body = self.req.body_mut();
        let bytes = boxbody_to_bytes(body).await?;
        if bytes.is_empty() {
            if let Some(data) = self.data.as_ref() {
                if let Some(typed_data) = data.downcast_ref::<T>() {
                    return Ok(Some(typed_data.clone()));
                }
                match self.try_convert::<T>(data) {
                    Ok(converted) => {
                        return Ok(Some(converted));
                    }
                    Err(e) => {
                        eprintln!("Conversion failed: {:?}", e);
                        return Ok(None);
                    }
                }
            }
            return Ok(None);
        }
        let body_str = String::from_utf8(bytes.to_vec())?;
        if !body_str.trim().is_empty() {
            let json_value: T = serde_json::from_str(&body_str)?;
            Ok(Some(json_value))
        } else {
            Ok(None)
        }
    }
    /// Attempts type conversion between supported formats (Vec<u8>, String, Json::Value)
    fn try_convert<T: std::any::Any + Send + Clone + DeserializeOwned + Debug>(
        &self,
        data: &Box<dyn std::any::Any + Send>,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(value_data) = data.downcast_ref::<serde_json::Value>() {
            return Ok(serde_json::from_value(value_data.clone())?);
        }

        if let Some(string_data) = data.downcast_ref::<String>() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(string_data) {
                let boxed_value: Box<dyn Any + Send> = Box::new(value);
                return self.try_convert::<T>(&boxed_value);
            }
            return Ok(serde_json::from_str(string_data)?);
        }

        if let Some(vec_data) = data.downcast_ref::<Vec<u8>>() {
            let s = String::from_utf8(vec_data.clone())?;
            let boxed_value: Box<dyn Any + Send> = Box::new(s);
            return self.try_convert::<T>(&boxed_value);
        }

        let from_type = std::any::type_name_of_val(data);
        let to_type = std::any::type_name::<T>();

        Err(format!(
            "Type conversion failed: cannot convert from '{}' to '{}'",
            from_type, to_type
        )
        .into())
    }
    pub fn set_data<T: std::any::Any + Send + Debug>(&mut self, data: T) {
        self.data = Some(Box::new(data));
    }
    pub fn params(&self) -> &Params {
        &self.params
    }
    pub fn params_mut(&mut self) -> &mut Params {
        &mut self.params
    }
}
#[derive(Debug, Default, Clone)]
pub struct Params {
    path_params: HashMap<String, String>,
    query_params: HashMap<String, Vec<String>>,
}

impl Params {
    pub fn new() -> Self {
        Self {
            path_params: HashMap::new(),
            query_params: HashMap::new(),
        }
    }

    pub fn get_path(&self, key: &str) -> Option<&String> {
        self.path_params.get(key)
    }

    pub fn get_query(&self, key: &str) -> Option<&Vec<String>> {
        self.query_params.get(key)
    }

    pub fn get_query_first(&self, key: &str) -> Option<&String> {
        self.query_params.get(key).and_then(|v| v.first())
    }
}
#[derive(Debug)]
pub struct Res {
    pub res: HandlerResult,
    pub headers: HeaderMap,
}

impl Default for Res {
    fn default() -> Self {
        Res {
            res: Ok(Response::new(BoxBody::new(http_body_util::Empty::new()))),
            headers: HeaderMap::new(),
        }
    }
}

impl Res {
    pub fn make_json_response<T: serde::Serialize>(data: T) -> Res {
        let body = serde_json::to_string(&data);
        match body {
            Ok(body) => {
                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(BoxBody::new(http_body_util::Full::new(body.into())))
                    .unwrap();
                Res {
                    res: Ok(response),
                    headers: HeaderMap::new(),
                }
            }
            Err(_) => {
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/json")
                    .body(BoxBody::new(http_body_util::Empty::new()))
                    .unwrap();
                Res {
                    res: Ok(response),
                    headers: HeaderMap::new(),
                }
            }
        }
    }
    pub fn set_headers(&mut self, headers: HeaderMap) {
        match &mut self.res {
            Ok(response) => {
                response.headers_mut().extend(headers.clone());
            }
            Err(e) => {
                eprintln!("Failed to set headers due to error: {:?}", e);
            }
        }
    }
    pub fn error<T: serde::Serialize>(data: T) -> Res {
        let body = serde_json::to_string(&data);
        match body {
            Ok(body) => {
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/json")
                    .body(BoxBody::new(http_body_util::Full::new(body.into())))
                    .unwrap();
                Res {
                    res: Ok(response),
                    headers: HeaderMap::new(),
                }
            }
            Err(_) => {
                let response = Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/json")
                    .body(BoxBody::new(http_body_util::Empty::new()))
                    .unwrap();
                Res {
                    res: Ok(response),
                    headers: HeaderMap::new(),
                }
            }
        }
    }
}
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub struct MiddlewareArc(Arc<dyn Middleware + Send + Sync>);
impl MiddlewareArc {
    pub fn extract(self) -> Arc<dyn Middleware + Send + Sync> {
        self.0
    }
}

impl Middleware for MiddlewareArc {
    fn handle(&self, req: Req, next: NextWrapper) -> BoxFuture<'static, Res> {
        self.0.handle(req, next)
    }
}
pub struct MiddlewareClosure<F> {
    func: Arc<F>,
}

impl<F> MiddlewareClosure<F>
where
    F: Fn(Req, NextWrapper) -> BoxFuture<'static, Res> + Send + Sync + 'static,
{
    pub fn new(func: F) -> Self {
        MiddlewareClosure {
            func: Arc::new(func),
        }
    }
}

impl<F> Middleware for MiddlewareClosure<F>
where
    F: Fn(Req, NextWrapper) -> BoxFuture<'static, Res> + Send + Sync + 'static,
{
    fn handle(&self, req: Req, next: NextWrapper) -> BoxFuture<'static, Res> {
        (self.func)(req, next)
    }
}

pub struct MiddlewareBuilder<F, M, D> {
    route_builder: RouteBuilder<F, D>,
    middleware: M,
}

impl<F, M, D> MiddlewareBuilder<F, M, D>
where
    F: Fn(
            Req,
            D,
            hyper::HeaderMap,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res> + Send + 'static>>
        + Send
        + Sync
        + 'static,
    M: Middleware + Send + Sync + 'static,
    D: DataType + Send + Sync + 'static + Clone,
{
    pub fn new(route_builder: RouteBuilder<F, D>, middleware: M) -> Self {
        Self {
            route_builder,
            middleware,
        }
    }

    pub fn build(self, server: &mut HttpServer) {
        if let Some(handler) = self.route_builder.handler {
            let middleware_arc = MiddlewareArc(Arc::new(self.middleware));
            let middleware = middleware_arc.extract();

            server.add_route(
                RouteBuilder {
                    method: self.route_builder.method,
                    path: self.route_builder.path,
                    handler: Some(handler),
                    content_header: self.route_builder.content_header,
                    _marker: self.route_builder._marker,
                },
                Some(middleware),
            );
        }
    }
}
pub trait Middleware {
    fn handle(&self, req: Req, next: NextWrapper) -> BoxFuture<'static, Res>;
}
impl Middleware for MiddlewareChain {
    fn handle(&self, req: Req, next: NextWrapper) -> BoxFuture<'static, Res> {
        let middlewares = Arc::from(self.middlewares.clone().into_boxed_slice());
        let handler = next.get_func().clone();
        Box::pin(next.call_last(req, middlewares, 0, handler))
    }
}
pub struct MiddlewareChain {
    middlewares: Vec<Arc<dyn Middleware + Send + Sync>>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        MiddlewareChain {
            middlewares: Vec::new(),
        }
    }

    pub fn chain<M>(&mut self, middleware: M)
    where
        M: Middleware + 'static + Send + Sync,
    {
        self.middlewares.push(Arc::new(middleware));
    }
}
pub struct Cors {
    allow_origin: String,
    allow_methods: String,
    allow_headers: String,
    allow_credentials: bool,
    expose_headers: String,
    max_age: String,
}
impl Cors {
    pub fn allow_origin(&self) -> &str {
        &self.allow_origin
    }

    pub fn allow_methods(&self) -> &str {
        &self.allow_methods
    }

    pub fn allow_headers(&self) -> &str {
        &self.allow_headers
    }

    pub fn allow_credentials(&self) -> bool {
        self.allow_credentials
    }

    pub fn expose_headers(&self) -> &str {
        &self.expose_headers
    }

    pub fn max_age(&self) -> &str {
        &self.max_age
    }
}

pub struct CorsBuilder {
    cors: Cors,
}

impl CorsBuilder {
    pub fn new() -> CorsBuilder {
        CorsBuilder {
            cors: Cors {
                allow_origin: String::new(),
                allow_methods: String::new(),
                allow_headers: String::new(),
                allow_credentials: false,
                expose_headers: String::new(),
                max_age: String::new(),
            },
        }
    }

    pub fn allow_origin(mut self, allow_origin: String) -> Self {
        self.cors.allow_origin = allow_origin;
        self
    }

    pub fn allow_methods(mut self, allow_methods: String) -> Self {
        self.cors.allow_methods = allow_methods;
        self
    }

    pub fn allow_headers(mut self, allow_headers: String) -> Self {
        self.cors.allow_headers = allow_headers;
        self
    }

    pub fn allow_credentials(mut self, allow_credentials: bool) -> Self {
        self.cors.allow_credentials = allow_credentials;
        self
    }

    pub fn expose_headers(mut self, expose_headers: String) -> Self {
        self.cors.expose_headers = expose_headers;
        self
    }

    pub fn max_age(mut self, max_age: String) -> Self {
        self.cors.max_age = max_age;
        self
    }

    pub fn build(self) -> Cors {
        self.cors
    }
}
