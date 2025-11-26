# apalis-pgmq
Background task processing in rust using apalis and pgmq

[![Crates.io](https://img.shields.io/crates/v/apalis-pgmq.svg)](https://crates.io/crates/apalis-pgmq)
[![Documentation](https://docs.rs/apalis-pgmq/badge.svg)](https://docs.rs/apalis-pgmq)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PostgreSQL storage backend for [Apalis](https://github.com/geofmureithi/apalis),
similar in spirit to [`apalis-sqlite`](https://github.com/apalis-dev/apalis-sqlite)
but using PostgreSQL as the database.

## Features

- **Persistent storage** – Jobs stored in PostgreSQL survive restarts
- **Retry support** – Failed jobs can be retried with configurable limits
- **Scheduled jobs** – Support for delayed job execution
- **Worker pools** – Multiple workers can process jobs concurrently
- **Custom codecs** – Control how jobs are serialized to bytes

## Prerequisites

- PostgreSQL 13+
- Rust 1.70 or later

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
apalis-pgmq = "0.1"
apalis-core = "1.0.0-beta.1"
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono", "json", "migrate"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Migrations

This crate ships with SQLx migrations under `migrations/`.

Make sure `DATABASE_URL` is set, then run:

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost/postgres
sqlx migrate run
```

You can also call `PostgresStorage::setup(&pool).await?` from your
application to run the embedded migrations at startup.

## Quick Start

```rust,no_run
use apalis_pgmq::{PostgresStorage, Backend, TaskSink, SqlContext};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use apalis_core::worker::builder::WorkerBuilder;
use apalis_core::Monitor;

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
    // Connect to PostgreSQL
    let pool = PgPool::connect("postgres://postgres:postgres@localhost/postgres").await?;

    // Run migrations (creates the jobs table, indexes, etc.)
    PostgresStorage::setup(&pool).await?;

    // Create storage for a given queue
    let storage = PostgresStorage::<EmailJob>::new(&pool);

    // Optionally push a job
    storage
        .sink()
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

The crate provides a PostgreSQL-specific acknowledgement layer that mirrors
`apalis-sqlite`:

- `PostgresAck` updates job status (done, failed, killed)
- `LockTaskLayer` ensures jobs are locked per worker using PostgreSQL
  queries under `queries/task/` (e.g. `lock_by_id.sql`, `ack.sql`)

This avoids panics when jobs are already processed and instead logs useful
diagnostics via `tracing`.

## Examples

You can adapt the snippet above
for your own job types and queues. Additional examples may live under the
`examples/` directory.

## License

MIT

## Contributing

Contributions are welcome! Please feel free to open issues or submit PRs.
