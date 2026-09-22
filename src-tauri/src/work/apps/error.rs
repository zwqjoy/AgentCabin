use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    NotFound(String),
    Storage(String),
    Provider(String),
    Validation(String),
    Unauthorized(String),
    Security(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {msg}"),
            AppError::Storage(msg) => write!(f, "Storage error: {msg}"),
            AppError::Provider(msg) => write!(f, "Provider error: {msg}"),
            AppError::Validation(msg) => write!(f, "Validation error: {msg}"),
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
            AppError::Security(msg) => write!(f, "Security violation: {msg}"),
            AppError::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Storage(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Storage(err.to_string())
    }
}
