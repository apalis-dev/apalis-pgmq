# apalis-pgmq

[![Crates.io](https://img.shields.io/crates/v/apalis-pgmq.svg)](https://crates.io/crates/apalis-pgmq)
[![Documentation](https://docs.rs/apalis-pgmq/badge.svg)](https://docs.rs/apalis-pgmq)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

PGMQ backend for [Apalis](https://github.com/apalis-dev/apalis), providing a PostgreSQL-based message queue for distributed job processing.

## Overview

This crate provides a message queue backend for Apalis using [PGMQ](https://github.com/tembo-io/pgmq) (PostgreSQL Message Queue). PGMQ is a lightweight, Postgres-based message queue with guaranteed delivery and visibility timeouts, similar to AWS SQS.

## Features

- **Persistent storage** – Messages stored in PostgreSQL survive restarts
- **Visibility timeout** – Messages become invisible during processing and reappear if not acknowledged
- **Guaranteed delivery** – Messages are delivered to exactly one consumer within the visibility timeout
- **Distributed processing** – Multiple workers can process messages concurrently
- **Auto-retry** – Failed messages automatically become visible again after timeout
- **Archive support** – Messages can be archived for long-term retention instead of deletion
- **Configurable polling** – Adjust poll intervals and visibility timeouts per queue

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
apalis-core = "1.0.0-beta.2"
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

### Creating a Backend

```rust
use apalis_pgmq::PgMq;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Job {
    id: u64,
    data: String,
}

// Create a PGMQ backend
let backend = PgMq::<Job>::new(
    "postgres://postgres:postgres@localhost:5432/postgres",
    "jobs"
)
.await?
.with_visibility_timeout(std::time::Duration::from_secs(60))
.with_poll_interval(std::time::Duration::from_millis(500))
.with_max_retries(5);
```

### Enqueuing Messages

```rust
// Enqueue a message
let msg_id = backend.enqueue(Job {
    id: 1,
    data: "process this".to_string(),
}).await?;

println!("Enqueued message with ID: {}", msg_id);
```

### Using with Apalis

Use the backend with Apalis `WorkerBuilder`:

```rust
use apalis_core::backend::Backend;

// The backend implements the Backend trait
// Use it with WorkerBuilder to create workers
let worker = WorkerBuilder::new("my-worker")
    .backend(backend)
    .build(my_handler_function);
```

## Configuration

### Visibility Timeout

Set how long messages stay invisible after being read (default: 30 seconds):

```rust
let backend = PgMq::<Job>::new(url, "queue")
    .await?
    .with_visibility_timeout(std::time::Duration::from_secs(120)); // 2 minutes
```

### Poll Interval

Set how frequently to poll for new messages (default: 100ms):

```rust
let backend = PgMq::<Job>::new(url, "queue")
    .await?
    .with_poll_interval(std::time::Duration::from_millis(500)); // 500ms
```

### Max Retries

Set maximum retry attempts before archiving (default: 5):

```rust
let backend = PgMq::<Job>::new(url, "queue")
    .await?
    .with_max_retries(3); // Archive after 3 failed attempts
```

## API Reference

### PgMq Methods

- `new(connection_url, queue_name)` - Create a new PGMQ backend
- `with_visibility_timeout(duration)` - Set visibility timeout
- `with_poll_interval(duration)` - Set poll interval
- `with_max_retries(count)` - Set max retry attempts
- `enqueue(job)` - Enqueue a message, returns message ID
- `ack(msg_id)` - Acknowledge (delete) a message
- `archive(msg_id)` - Archive a message
- `retry(msg_id, read_count)` - Retry logic (archives if max retries exceeded)

### PgMqContext

The context provides message metadata:

```rust
pub struct PgMqContext {
    pub msg_id: Option<i64>,
    pub read_ct: Option<i32>,
    pub enqueued_at: Option<chrono::DateTime<chrono::Utc>>,
    pub vt: Option<chrono::DateTime<chrono::Utc>>,
}
```

## Message Acknowledgment

Messages are automatically acknowledged by Apalis when your job handler completes successfully. If the handler returns an error or panics, the message will become visible again after the visibility timeout expires.

## Key Differences from Storage Backends

Unlike storage backends (like `apalis-sql`), message queues:

1. **Are immutable** – Messages cannot be updated after being enqueued
2. **Have visibility timeouts** – Messages become temporarily invisible when read
3. **Auto-requeue on failure** – Unacknowledged messages automatically return to the queue
4. **No job state tracking** – Messages are either in the queue or processed (no "running" state)
5. **Simpler semantics** – Focus on message delivery rather than job lifecycle management

## Comparison with apalis-sql

| Feature | apalis-pgmq | apalis-sql |
|---------|-------------|------------|
| Backend | PGMQ | Direct SQL |
| Message mutability | Immutable | Mutable |
| Visibility timeout | Yes | No |
| Auto-requeue | Yes | Manual |
| Job state tracking | No | Yes (pending/running/done) |
| Archive support | Yes | No |
| Use case | Message queues | Job queues with state |
| Complexity | Lower | Higher |

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Apalis](https://github.com/apalis-dev/apalis) - Simple, extensible multithreaded background job processing library for Rust
- [PGMQ](https://github.com/tembo-io/pgmq) - PostgreSQL Message Queue
