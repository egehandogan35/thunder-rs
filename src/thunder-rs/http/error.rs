use std::convert::Infallible;
use std::string;
#[derive(Debug)]
pub enum HttpError {
    Hyper(hyper::Error),
    Utf8(std::string::FromUtf8Error),
    IoError(std::io::Error),
    STR(string::String),
    Message(String),
    DataConversionError,
    JsonParsingError,
    RequestCreationError,
    BodyCreationError,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HttpError::Hyper(err) => write!(f, "Hyper error: {}", err),
            HttpError::Utf8(err) => write!(f, "Utf8 error: {}", err),
            HttpError::Message(msg) => write!(f, "Message: {}", msg),
            HttpError::IoError(err) => write!(f, "IoError: {}", err),
            HttpError::STR(err) => write!(f, "STR: {}", err),
            HttpError::DataConversionError => write!(f, "Data conversion error"),
            HttpError::JsonParsingError => write!(f, "Json parsing error"),
            HttpError::RequestCreationError => write!(f, "Request creation error"),
            HttpError::BodyCreationError => write!(f, "Body creation error"),
        }
    }
}

impl std::error::Error for HttpError {}

impl From<Infallible> for HttpError {
    fn from(_: Infallible) -> HttpError {
        HttpError::Message("Infallible".to_string())
    }
}

impl From<hyper::Error> for HttpError {
    fn from(err: hyper::Error) -> HttpError {
        HttpError::Hyper(err)
    }
}

impl From<std::string::FromUtf8Error> for HttpError {
    fn from(err: std::string::FromUtf8Error) -> HttpError {
        HttpError::Utf8(err)
    }
}
impl From<String> for HttpError {
    fn from(msg: String) -> HttpError {
        HttpError::STR(msg.to_string())
    }
}
impl From<std::io::Error> for HttpError {
    fn from(err: std::io::Error) -> HttpError {
        HttpError::IoError(err)
    }
}
impl From<hyper::header::InvalidHeaderValue> for HttpError {
    fn from(err: hyper::header::InvalidHeaderValue) -> HttpError {
        HttpError::new(&format!("Invalid header value: {}", err))
    }
}

impl HttpError {
    pub fn new(msg: &str) -> HttpError {
        HttpError::Message(msg.to_string())
    }
}
