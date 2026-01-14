# apalis-pgmq

[![Crates.io](https://img.shields.io/crates/v/apalis-pgmq.svg)](https://crates.io/crates/apalis-pgmq)
[![Documentation](https://docs.rs/apalis-pgmq/badge.svg)](https://docs.rs/apalis-pgmq)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PGMQ backend for [Apalis](https://github.com/apalis-dev/apalis), providing a PostgreSQL-based message queue for distributed message processing.

## Overview

This crate provides a message queue backend for Apalis using [PGMQ](https://github.com/tembo-io/pgmq) (PostgreSQL Message Queue). PGMQ is a lightweight, Postgres-based message queue with guaranteed delivery and visibility timeouts, similar to AWS SQS.

## Features

- **Persistent storage** – Messages stored in PostgreSQL survive restarts
- **Visibility timeout** – Messages become invisible during processing and reappear if not acknowledged
- **Guaranteed delivery** – Messages are delivered to exactly one consumer within the visibility timeout
- **Distributed processing** – Multiple workers can process messages concurrently
- **Auto-retry** – Failed messages automatically become visible again after timeout
- **Configurable polling** – Adjust poll intervals and visibility timeouts per queue
- **Codec-based serialization** – Uses apalis-codec for flexible message encoding

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
apalis = "1.0.0-rc.2"
apalis-pgmq = "0.1"
serde = { version = "1", features = ["derive"] }
```

## Prerequisites

You need PostgreSQL 13+ with the PGMQ extension installed. Follow the [PGMQ installation guide](https://github.com/tembo-io/pgmq#installation).

For development, you can use Docker:

```bash
docker run -d --name postgres \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:15

# Install PGMQ extension
docker exec -it postgres psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;"
```

## Usage

### Basic Example

```rust
use apalis::prelude::*;
use apalis_pgmq::{PgMqBackend, Config};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmailMessage {
    to: String,
    subject: String,
    body: String,
}

async fn send_email(message: EmailMessage) -> Result<(), std::io::Error> {
    // Process the message
    println!("Sending email to: {}", message.to);
    Ok(())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Create a PGMQ backend
    let backend = PgMqBackend::<EmailMessage>::new(
        "postgres://postgres:postgres@localhost:5432/postgres",
        "emails"
    ).await?;

    // Build and run worker
    Monitor::new()
        .register(
            WorkerBuilder::new("email-worker")
                .backend(backend)
                .build_fn(send_email)
        )
        .run()
        .await?;

    Ok(())
}

// Use your preferred async runtime (tokio, async-std, smol, etc.)
fn main() {
    // Example with tokio:
    // tokio::runtime::Runtime::new().unwrap().block_on(run()).unwrap();
    
    // Example with async-std:
    // async_std::task::block_on(run()).unwrap();
}
```

### Custom Configuration

```rust
use apalis_pgmq::Config;
use std::time::Duration;

// Create custom configuration
let config = Config::new("my-queue")
    .with_visibility_timeout(Duration::from_secs(120))
    .with_poll_interval(Duration::from_millis(500))
    .with_max_retries(3);

let backend = PgMqBackend::<EmailMessage>::new_with_config(
    "postgres://postgres:postgres@localhost:5432/postgres",
    config
).await?;
```

## Configuration

The `Config` struct provides configuration options for the PGMQ backend:

```rust
use apalis_pgmq::Config;
use std::time::Duration;

let config = Config::new("my-queue")
    .with_visibility_timeout(Duration::from_secs(120))  // Default: 30s
    .with_poll_interval(Duration::from_millis(500))     // Default: 100ms
    .with_max_retries(3);                               // Default: 5
```

### Configuration Options

- **`visibility_timeout`** - How long messages stay invisible after being read (default: 30 seconds)
- **`poll_interval`** - How frequently to poll for new messages when queue is empty (default: 100ms)
- **`max_retries`** - Maximum retry attempts before message handling fails (default: 5)

## API Reference

### PgMqBackend

```rust
impl<T> PgMqBackend<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    pub async fn new(connection_url: &str, queue_name: &str) -> Result<Self, PgMqError>
    pub async fn new_with_config(connection_url: &str, config: Config) -> Result<Self, PgMqError>
    pub fn queue(&self) -> &PGMQueue
    pub fn config(&self) -> &Config
}
```

### Config

```rust
impl Config {
    pub fn new(namespace: impl Into<String>) -> Self
    pub fn with_visibility_timeout(self, timeout: Duration) -> Self
    pub fn with_poll_interval(self, interval: Duration) -> Self
    pub fn with_max_retries(self, max_retries: i32) -> Self
    pub fn namespace(&self) -> &str
    pub fn visibility_timeout(&self) -> Duration
    pub fn poll_interval(&self) -> Duration
    pub fn max_retries(&self) -> i32
}
```

### Backend Trait

`PgMqBackend<T>` implements the `Backend` trait from `apalis-core`:

- **`type Args = T`** - Your message type
- **`type IdType = i64`** - PGMQ message IDs
- **`type Context = ()`** - No additional context
- **`type Error = PgMqError`** - Error type

## Message Processing

Messages are processed by Apalis workers. When a worker successfully processes a message, PGMQ automatically removes it from the queue. If processing fails or the worker crashes, the message becomes visible again after the visibility timeout expires and can be retried.

### Serialization

This backend uses `JsonCodec` from `apalis-codec` for message serialization. Your message types must implement `Serialize` and `Deserialize` from serde:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyMessage {
    // your fields
}
```

## Key Characteristics

This is a **message queue** backend, not a storage backend:

1. **Immutable messages** – Messages cannot be updated after being enqueued
2. **Visibility timeouts** – Messages become temporarily invisible when read
3. **Auto-requeue** – Unacknowledged messages automatically return to the queue
4. **No state tracking** – Messages are either in the queue or processed
5. **Simple semantics** – Focus on message delivery, not job lifecycle

## Comparison with Storage Backends

| Feature | apalis-pgmq | apalis-sql |
|---------|-------------|------------|
| Type | Message Queue | Storage |
| Backend | PGMQ | Direct SQL |
| Mutability | Immutable | Mutable |
| Visibility timeout | Yes | No |
| Auto-requeue | Yes | Manual |
| State tracking | No | Yes |
| Use case | Message processing | Job management |

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Apalis](https://github.com/apalis-dev/apalis) - Simple, extensible multithreaded background job processing library for Rust
- [PGMQ](https://github.com/tembo-io/pgmq) - PostgreSQL Message Queue
