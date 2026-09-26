//! Custom errors types for apalis-pgmq
use pgmq::PgmqError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    /// Inner pgmq error
    #[error("inner pgmq error {0}")]
    Inner(#[from] PgmqError),

    /// general query error
    #[error("query error {0}")]
    Database(#[from] sqlx::Error),
}
