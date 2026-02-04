# apalis-pgmq

Background task processing in rust using apalis and pgmq

## Features

- **Reliable message queue** using `pgmq` as the backend.
- **Multiple storage types**: standard polling and `trigger` based storages.
- **Custom codecs** for serializing/deserializing job arguments as bytes.
- **Integration with `apalis` workers and middleware.**
- **Observability**: Monitor and manage tasks using [apalis-board](https://github.com/apalis-dev/apalis-board).

## Examples

### Setting up

The fastest way to get started is by running the Docker image, where PGMQ comes pre-installed in Postgres.

```sh
docker run -d --name pgmq-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 ghcr.io/pgmq/pg18-pgmq:v1.7.0
```

### Basic Worker Example

```rust,no_run
use apalis::prelude::*;
use apalis_pgmq::*;
use futures::stream::{self, StreamExt, SinkExt};

#[tokio::main]
async fn main() {
   let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let mut backend = PGMQueue::new(pool, "default_queue").await;

    backend.send(Task::new(HashMap::new())).await.unwrap();

    async fn send_reminder(
        msg: HashMap<String, String>,
        wrk: WorkerContext,
    ) -> Result<(), BoxDynError> {
        Ok(())
    }

    let worker = WorkerBuilder::new("rango-tango-1")
        .backend(backend)
        .build(send_reminder);
    worker.run().await.unwrap();
}
```

## Observability

Track your jobs using [apalis-board](https://github.com/apalis-dev/apalis-board).
![Task](https://github.com/apalis-dev/apalis-board/raw/main/screenshots/task.png)

## Roadmap

- [x] Eager Fetcher
- [ ] Lazy Fetcher (using NOTIFY)
- [ ] Shared Fetcher (Multiple queues on the same connection)
- [x] Batch Sink
- [ ] BackendExt
- [ ] Worker heartbeats
- [ ] Workflow support
- [ ] Extensive Docs
- [ ] Maximize compatibility with [pgmq](https://github.com/pgmq/pgmq)

## Comparison with pgmq

Our version of `pgmq` differs in several ways to offer better support for `apalis`:

1. Messages are stored as `BYTEA` instead of `JSONB` to offer better codec support.
2. Uses `headers` which is not yet supported in the rs version
3. Uses the `apalis_pgmq` schema instead of `pgmq`.

## Credits

- [pgmq](https://github.com/pgmq/pgmq) :A lightweight message queue.

## License

Licensed under Postgres License.
