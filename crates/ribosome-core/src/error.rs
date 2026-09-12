use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[error("{message}")]
pub struct Error {
    pub code: i32,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }
    pub fn denied(message: impl Into<String>) -> Self {
        Self {
            code: -32001,
            message: message.into(),
        }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: -32002,
            message: message.into(),
        }
    }
    pub fn missing(message: impl Into<String>) -> Self {
        Self {
            code: -32004,
            message: message.into(),
        }
    }
    pub fn exhausted(message: impl Into<String>) -> Self {
        Self {
            code: -32005,
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::internal(value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::invalid(value.to_string())
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::internal(value.to_string())
    }
}
