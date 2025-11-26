# apalis-pgmq
Background task processing in Rust using Apalis and PGMQ

[![Crates.io](https://img.shields.io/crates/v/apalis-pgmq.svg)](https://crates.io/crates/apalis-pgmq)
[![Documentation](https://docs.rs/apalis-pgmq/badge.svg)](https://docs.rs/apalis-pgmq)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PGMQ (Postgres Message Queue) storage backend for [Apalis](https://github.com/geofmureithi/apalis),
using [PGMQ](https://github.com/pgmq/pgmq) for message queue operations.

## Features

- **PGMQ Integration** – Uses PGMQ's native queue operations for efficient message handling
- **Persistent storage** – Jobs stored in PGMQ queues survive restarts
- **Retry support** – Failed jobs can be retried with configurable limits
- **Visibility timeout** – Built-in message locking via PGMQ's visibility timeout
- **Worker pools** – Multiple workers can process jobs concurrently
- **Message archiving** – Archive completed messages for long-term retention
- **Custom codecs** – Control how jobs are serialized to bytes
- **Guaranteed delivery** – Messages delivered to exactly one consumer within visibility timeout

## Prerequisites

- PostgreSQL 13+
- Rust 1.70 or later

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
apalis-pgmq = "0.1"
apalis-core = "1.0.0-beta.1"
pgmq = "0.31"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

## Setup

PGMQ uses a pure Rust client and doesn't require any PostgreSQL extensions.
Just start PostgreSQL:

```bash
docker run -d --name postgres \
  -e POSTGRES_PASSWORD=postgres \
  -p 5432:5432 \
  postgres:15
```

PGMQ will automatically create the necessary queue tables when you initialize the storage.

## Quick Start

```rust,no_run
use apalis_pgmq::{PgmqStorage, SqlContext};
use apalis_core::backend::TaskSink;
use apalis_core::worker::builder::WorkerBuilder;
use apalis_core::Monitor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmailJob {
    to: String,
    subject: String,
    body: String,
}

async fn send_email(job: EmailJob, _ctx: SqlContext) {
    println!("Sending email to: {}", job.to);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = "postgres://postgres:postgres@localhost/postgres";

    // Create PGMQ storage for a given queue
    let mut storage = PgmqStorage::<EmailJob>::new(db_url, "email_queue").await?;

    // Push a job
    storage
        .push(EmailJob {
            to: "user@example.com".into(),
            subject: "Hello".into(),
            body: "World".into(),
        })
        .await?;

    // Build and run a worker
    let worker = WorkerBuilder::new("email-worker")
        .backend(storage)
        .build(send_email);

    Monitor::new().register(worker).run().await?;
    Ok(())
}
```

## Acknowledgement & Locking

The crate provides PGMQ-specific acknowledgement operations:

- `PgmqAck` handles message acknowledgement using PGMQ operations:
  - **Done** – Archives successful messages using `queue.archive()`
  - **Killed** – Deletes permanently failed messages using `queue.delete()`
  - **Failed** – Leaves retryable failures for visibility timeout to expire
- `LockTaskLayer` is provided for API compatibility (PGMQ handles locking via visibility timeout)

PGMQ's visibility timeout mechanism ensures messages are locked per worker automatically,
avoiding the need for explicit database locks.

## Examples

You can adapt the snippet above
for your own job types and queues. Additional examples may live under the
`examples/` directory.

## License

MIT

## Contributing

Contributions are welcome! Please feel free to open issues or submit PRs.
