use thiserror::Error;

/// Errors that can occur when using the PGMQ backend.
#[derive(Debug, Error)]
pub enum PgMqError {
    /// An error from the underlying PGMQ library.
    #[error("PGMQ error: {0}")]
    Pgmq(#[from] pgmq::errors::PgmqError),
}