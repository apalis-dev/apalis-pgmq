//! # Apalis PGMQ
//!
//! PGMQ (Postgres Message Queue) storage backend for [Apalis](https://github.com/geofmureithi/apalis).
//!
//! This crate provides a persistent, PGMQ-based job queue backend for Apalis,
//! leveraging PGMQ's optimized queue operations built on PostgreSQL.
//!
//! ## Features
//!
//! - **PGMQ Integration**: Uses PGMQ's native queue operations for efficient message handling
//! - **Persistent Storage**: Jobs stored in PGMQ queues survive application restarts
//! - **Retry Support**: Failed jobs can be retried with configurable limits
//! - **Visibility Timeout**: Built-in message locking via PGMQ's visibility timeout
//! - **Worker Pools**: Multiple workers can process jobs concurrently
//! - **Message Archiving**: Archive completed messages for long-term retention
//! - **Custom Codecs**: Serialize/deserialize job arguments as bytes
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use apalis_pgmq::PgmqStorage;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct EmailJob {
//!     to: String,
//!     subject: String,
//!     body: String,
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db_url = "postgres://postgres:postgres@localhost/postgres";
//!     
//!     // Create PGMQ storage
//!     let storage = PgmqStorage::<EmailJob>::new(db_url, "email_queue").await?;
//!     
//!     Ok(())
//! }
//! ```

mod backend;
mod config;
mod error;
mod sink;
mod ack;

pub use backend::{PgmqStorage, PgmqTask, CompactType};
pub use config::Config;
pub use error::{PgmqError, Result};
pub use ack::{PgmqAck, LockTaskLayer};

pub use apalis_core::backend::{Backend, TaskSink};
pub use apalis_sql::context::SqlContext;
pub use pgmq::{PGMQueue, Message};
