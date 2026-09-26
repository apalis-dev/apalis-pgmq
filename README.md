# apalis-pgmq

Background task processing in rust using apalis and pgmq

## Features

- **Reliable message queue** using `pgmq` as the backend.
- **Multiple storage types**: standard polling and `trigger` based polling.
- **Custom codecs** for serializing/deserializing job arguments as bytes.
- **Integration with `apalis` workers and middleware.**
- **Observability**: Monitor and manage tasks using [apalis-board](https://github.com/apalis-dev/apalis-board).

## Examples

### Setting up

The fastest way to get started is by running the Docker image, where PGMQ comes pre-installed in Postgres.

```sh
docker run -d --name pgmq-postgres -e POSTGRES_PASSWORD=postgres -p 5432:5432 ghcr.io/pgmq/pg18-pgmq:v1.10.0
```

Then connect and enable PGMQ:

```sh
psql postgres://postgres:postgres@localhost:5432/postgres
```

```sh
postgres=# CREATE EXTENSION pgmq;
```

### Basic Worker Example

```rust
use std::{collections::HashMap, env};

use apalis::prelude::*;
use apalis_pgmq::*;

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let mut backend = PGMQueue::new(pool);

    backend.push(42).await.unwrap();

    async fn send_reminder(
        _msg: usize,
        wrk: WorkerContext,
    ) -> Result<(), BoxDynError> {
        wrk.stop()?;
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
- [x] Lazy Fetcher (using NOTIFY)
- [x] Batch Sink
- [x] Bytes compatibility
- [x] Worker heartbeats
- [x] Workflow support
- [x] Extensive Docs
- [ ] Apalis board support
- [x] Maximize compatibility with [pgmq](https://github.com/pgmq/pgmq)

## Compatibility with pgmq

By default `apalis` recommends storing args as bytes. This allows features such as custom codecs, encryption and compression.

`apalis-pgmq` offers the ability to use bytes at the expense of compatibility.

If you turn on the `bytes-compat`, a new schema `apalis_pgmq` is created with bytes support.

If you want to use the bytes feature with sql commands use `apalis_pgmq` schema instead of `pgmq`

eg:

```sql
SELECT apalis_pgmq.create($1);
```

**Note**: The `bytes-compat` feature is not stable and is currently a patch. Use it only when necessary and you understand future releases will break until the json<->bytes issue is solved upstream


## Credits

- [pgmq](https://github.com/pgmq/pgmq) :A lightweight message queue.

## License

Licensed under Postgres License.
