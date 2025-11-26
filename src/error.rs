use thiserror::Error;

pub type Result<T> = std::result::Result<T, PostgresError>;

#[derive(Debug, Error)]
pub enum PostgresError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Task with ID {0} not found")]
    TaskNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Backend is closed")]
    Closed,

    #[error("Other error: {0}")]
    Other(String),
}

impl From<String> for PostgresError {
    fn from(s: String) -> Self {
        PostgresError::Other(s)
    }
}

impl From<&str> for PostgresError {
    fn from(s: &str) -> Self {
        PostgresError::Other(s.to_string())
    }
}
