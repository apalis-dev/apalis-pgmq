use std::{
    env,
    time::{Duration, Instant},
};

use apalis::prelude::*;
use apalis_pgmq::*;
use futures::{StreamExt, future::ready};
use sqlx::postgres::PgListener;

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    let mut listener = PgListener::connect_with(&pool).await.unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let config = Config::default().queue("basic");

    // Only necessary if the queue doesn't exist
    PGMQueue::create(&pool, &config).await.unwrap();

    let query = format!("SELECT {PGMQ_SCHEMA}.enable_notify_insert($1, $2)");

    sqlx::query(&query)
        .bind(config.queue.as_ref())
        .bind(250)
        .execute(&pool)
        .await
        .unwrap();

    listener
        .listen(&format!("{PGMQ_SCHEMA}.q_basic.INSERT"))
        .await
        .unwrap();

    let subscription = listener.into_stream().filter(|a| match a {
        Ok(not) => ready(not.channel().ends_with("q_basic.INSERT")),
        Err(_) => ready(false),
    });

    let backend = PGMQueue::new(pool)
        .with_config(config)
        .poll_with_stream(subscription);
    let mut b = backend.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        b.push(42usize).await.unwrap();
    });

    async fn send_reminder(_msg: usize, wrk: WorkerContext) -> Result<(), BoxDynError> {
        wrk.stop()?;
        Ok(())
    }

    let instant = Instant::now();
    let worker = WorkerBuilder::new("rango-tango-1")
        .backend(backend)
        .build(send_reminder);
    worker.run().await.unwrap();

    assert!(
        instant.elapsed() < Duration::from_millis(3000),
        "Worker run longer than expected"
    );
}
