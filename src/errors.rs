use pgmq::PgmqError as PgmqCrateError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PgMqError {
    #[error("PGMQ error: {0}")]
    Pgmq(#[from] PgmqCrateError),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Queue operation error: {0}")]
    QueueOperation(String),
}