use std::{collections::HashMap, env};

use apalis::prelude::*;
use apalis_pgmq::*;
use futures::{self, SinkExt};

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let mut backend = PGMQueue::new(pool, "basic").await;

    backend.send(Task::new(HashMap::new())).await.unwrap();

    async fn send_reminder(
        _msg: HashMap<String, String>,
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
