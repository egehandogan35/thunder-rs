use std::{fmt, io};
use tokio::task::JoinError;
#[derive(Debug, Clone)]
pub enum ErrorType {
    Recoverable,
    NonRecoverable,
}
/// WebSocket error handling based on RFC 6455
/// Errors are categorized as Recoverable or NonRecoverable to support automated recovery
/// https://datatracker.ietf.org/doc/html/rfc6455#section-7.4
///
/// TODO: Implement automated fallback/retry logic for Recoverable errors:
/// - Going Away (1001): Wait and retry
/// - Broken Pipe (32): Retry connection
/// - Unexpected EOF (10054): Retry with timeout
///
/// Note:
/// - Normal Closure (1000) is intentionally excluded as it represents
///   a clean shutdown that doesn't require reconnection
/// - Abnormal Closure (1006) is a reserved value that indicates an abnormal
///   closure without a proper Close frame. The actual recovery strategy
///   should be based on the underlying cause (32, 10054, etc.)

#[derive(Debug, Clone)]
pub struct Error {
    pub code: u16,
    pub message: &'static str,
    pub error_type: ErrorType,
}
impl From<JoinError> for Error {
    fn from(_: JoinError) -> Self {
        INTERNAL_ERROR
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (code: {})", self.message, self.code)
    }
}
impl std::error::Error for Error {}
pub const NORMAL_CLOSURE: Error = Error {
    code: 1000,
    message: "Normal Closure",
    error_type: ErrorType::Recoverable,
};
pub const GOING_AWAY: Error = Error {
    code: 1001,
    message: "Going Away",
    error_type: ErrorType::Recoverable,
};
pub const PROTOCOL_ERROR: Error = Error {
    code: 1002,
    message: "Protocol Error",
    error_type: ErrorType::NonRecoverable,
};
pub const UNSUPPORTED_DATA: Error = Error {
    code: 1003,
    message: "Unsupported Data",
    error_type: ErrorType::NonRecoverable,
};
pub const NO_STATUS_RECEIVED: Error = Error {
    code: 1005,
    message: "No Status Received",
    error_type: ErrorType::Recoverable,
};
pub const ABNORMAL_CLOSURE: Error = Error {
    code: 1006,
    message: "Abnormal Closure",
    error_type: ErrorType::Recoverable,
};
pub const INVALID_FRAME_PAYLOAD_DATA: Error = Error {
    code: 1007,
    message: "Invalid Frame Payload Data",
    error_type: ErrorType::NonRecoverable,
};
pub const POLICY_VIOLATION: Error = Error {
    code: 1008,
    message: "Policy Violation",
    error_type: ErrorType::NonRecoverable,
};
pub const MESSAGE_TOO_BIG: Error = Error {
    code: 1009,
    message: "Message Too Big",
    error_type: ErrorType::NonRecoverable,
};
pub const MISSING_EXTENSION: Error = Error {
    code: 1010,
    message: "Missing Extension",
    error_type: ErrorType::NonRecoverable,
};
pub const INTERNAL_ERROR: Error = Error {
    code: 1011,
    message: "Internal Error",
    error_type: ErrorType::NonRecoverable,
};
pub const TLS_HANDSHAKE: Error = Error {
    code: 1015,
    message: "TLS Handshake",
    error_type: ErrorType::NonRecoverable,
};
pub const BROKEN_PIPE: Error = Error {
    code: 32,
    message: "Broken pipe",
    error_type: ErrorType::Recoverable,
};
pub const UNEXPECTED_EOF: Error = Error {
    code: 10054,
    message: "Unexpected EOF",
    error_type: ErrorType::Recoverable,
};

pub enum Errors {
    NormalClosure(Error),
    GoingAway(Error),
    ProtocolError(Error),
    UnsupportedData(Error),
    NoStatusReceived(Error),
    AbnormalClosure(Error),
    InvalidFramePayloadData(Error),
    PolicyViolation(Error),
    MessageTooBig(Error),
    MissingExtension(Error),
    InternalError(Error),
    TLSHandshake(Error),
    BrokenPipe(Error),
    UnexpectedEOF(Error),
}

pub fn log_error(error: Errors) {
    let error = match error {
        Errors::NormalClosure(_error) => NORMAL_CLOSURE,
        Errors::GoingAway(_error) => GOING_AWAY,
        Errors::ProtocolError(_error) => PROTOCOL_ERROR,
        Errors::UnsupportedData(_error) => UNSUPPORTED_DATA,
        Errors::NoStatusReceived(_error) => NO_STATUS_RECEIVED,
        Errors::AbnormalClosure(_error) => ABNORMAL_CLOSURE,
        Errors::InvalidFramePayloadData(_error) => INVALID_FRAME_PAYLOAD_DATA,
        Errors::PolicyViolation(_error) => POLICY_VIOLATION,
        Errors::MessageTooBig(_error) => MESSAGE_TOO_BIG,
        Errors::MissingExtension(_error) => MISSING_EXTENSION,
        Errors::InternalError(_error) => INTERNAL_ERROR,
        Errors::TLSHandshake(_error) => TLS_HANDSHAKE,
        Errors::BrokenPipe(_error) => BROKEN_PIPE,
        Errors::UnexpectedEOF(_error) => UNEXPECTED_EOF,
    };
    println!("Error: {}", error.message);
    match error.error_type {
        ErrorType::Recoverable => println!("Recoverable"),
        ErrorType::NonRecoverable => println!("Non-Recoverable"),
    }
}
#[derive(Debug)]
pub struct SendError {
    pub error: io::Error,
    pub data: Vec<u8>,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Failed to send data: {:?}", self.data)
    }
}

impl std::error::Error for SendError {}

impl From<io::Error> for SendError {
    fn from(error: io::Error) -> Self {
        SendError {
            error,
            data: Vec::new(),
        }
    }
}

impl From<JoinError> for SendError {
    fn from(error: JoinError) -> Self {
        SendError {
            error: io::Error::new(io::ErrorKind::Other, error.to_string()),
            data: Vec::new(),
        }
    }
}

impl From<Error> for SendError {
    fn from(error: Error) -> Self {
        SendError {
            error: io::Error::new(io::ErrorKind::Other, error.message),
            data: Vec::new(),
        }
    }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(_: std::string::FromUtf8Error) -> Self {
        INVALID_FRAME_PAYLOAD_DATA
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::InvalidData => INVALID_FRAME_PAYLOAD_DATA,
            _ => INTERNAL_ERROR,
        }
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> io::Error {
        io::Error::new(io::ErrorKind::Other, error.message)
    }
}
