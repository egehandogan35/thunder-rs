pub mod routes;
pub mod server;
pub mod error;
pub mod httpmethod;
pub mod stfile;
use error::HttpError;
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::StatusCode;
use hyper::Response;
use std::convert::Infallible;

pub type HandlerResult = Result<Response<BoxBody<Bytes, Infallible>>, HttpError>;