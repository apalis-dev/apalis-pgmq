use std::env;

use apalis::prelude::*;
use apalis_pgmq::*;

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let config = Config::default().queue("basic");
    let mut backend = PGMQueue::new(pool).with_config(config);

    backend.push(42usize).await.unwrap();

    async fn send_reminder(_msg: usize, wrk: WorkerContext) -> Result<(), BoxDynError> {
        wrk.stop()?;
        Ok(())
    }

    let worker = WorkerBuilder::new("rango-tango-1")
        .backend(backend)
        .build(send_reminder);
    worker.run().await.unwrap();
}
