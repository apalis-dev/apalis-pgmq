//! # Apalis PostgreSQL
//!
//! PostgreSQL storage backend for [Apalis](https://github.com/geofmureithi/apalis).
//!
//! This crate provides a persistent, PostgreSQL-based job queue backend for Apalis,
//! similar to how apalis-sqlite works but using PostgreSQL as the database.
//!
//! ## Features
//!
//! - **Persistent Storage**: Jobs are stored in PostgreSQL and survive application restarts
//! - **Retry Support**: Failed jobs can be retried with configurable limits
//! - **Scheduled Jobs**: Support for delayed job execution
//! - **Worker Pools**: Multiple workers can process jobs concurrently
//! - **Custom Codecs**: Serialize/deserialize job arguments as bytes
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use apalis_postgres::PostgresStorage;
//! use sqlx::PgPool;
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
//!     // Connect to PostgreSQL
//!     let pool = PgPool::connect("postgres://postgres:postgres@localhost/postgres").await?;
//!     
//!     // Run migrations
//!     PostgresStorage::setup(&pool).await?;
//!     
//!     // Create storage
//!     let storage = PostgresStorage::new(&pool);
//!     
//!     Ok(())
//! }
//! ```

mod backend;
mod config;
mod convert;
mod error;
mod sink;
mod ack;

pub use backend::{PostgresStorage, PostgresTask, CompactType};
pub use config::Config;
pub use error::{PostgresError, Result};
pub use ack::{PostgresAck, LockTaskLayer};

pub use apalis_core::backend::{Backend, TaskSink};
pub use apalis_sql::context::SqlContext;
pub use sqlx::{PgPool, Pool, Postgres};
