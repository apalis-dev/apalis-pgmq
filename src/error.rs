use thiserror::Error;

pub type Result<T> = std::result::Result<T, PgmqError>;

#[derive(Debug, Error)]
pub enum PgmqError {
    #[error("PGMQ error: {0}")]
    Pgmq(#[from] pgmq::errors::PgmqError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Task with ID {0} not found")]
    TaskNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Backend is closed")]
    Closed,

    #[error("No message available")]
    NoMessage,

    #[error("Other error: {0}")]
    Other(String),
}

impl From<String> for PgmqError {
    fn from(s: String) -> Self {
        PgmqError::Other(s)
    }
}

impl From<&str> for PgmqError {
    fn from(s: &str) -> Self {
        PgmqError::Other(s.to_string())
    }
}
