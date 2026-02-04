use std::{marker::PhantomData, sync::Arc};

use apalis_codec::json::JsonCodec;
use apalis_core::{
    backend::{
        Backend, TaskStream,
        codec::Codec,
        poll_strategy::{PollContext, PollStrategyExt},
    },
    task::{Task, attempt::Attempt, task_id::TaskId},
    worker::{context::WorkerContext, ext::ack::AcknowledgeLayer},
};
use chrono::{DateTime, Utc};
use futures::{
    StreamExt,
    stream::{self, BoxStream},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Postgres};

use crate::{
    config::Config, context::PgMqContext, errors::PgmqError, fetch::fetch_messages, sink::PgMqSink,
};

mod ack;
mod config;
mod context;
mod errors;
mod fetch;
pub mod query;
mod sink;
mod util;

pub const QUEUE_PREFIX: &str = r#"q"#;
pub const ARCHIVE_PREFIX: &str = r#"a"#;
pub const PGMQ_SCHEMA: &str = "apalis_pgmq";

pub type PgMqTask<Args> = Task<Args, PgMqContext, i64>;

pub struct PGMQueue<Args, Codec = JsonCodec<Vec<u8>>> {
    connection: PgPool,
    config: Config<Codec>,
    sink: PgMqSink<Args, Codec>,
    _args: PhantomData<Args>,
}

impl<Args, C> Clone for PGMQueue<Args, C> {
    fn clone(&self) -> Self {
        Self {
            connection: self.connection.clone(),
            config: self.config.clone(),
            sink: self.sink.clone(),
            _args: self._args,
        }
    }
}

impl PGMQueue<()> {
    pub async fn setup<'c, E: sqlx::Executor<'c, Database = Postgres>>(
        executor: E,
    ) -> Result<bool, PgmqError> {
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pgmq CASCADE;")
            .execute(executor)
            .await
            .map(|_| true)
            .map_err(PgmqError::from)
    }
    async fn create<'c, E>(queue_name: &str, executor: E) -> Result<(), PgmqError>
    where
        E: sqlx::Acquire<'c, Database = Postgres>,
    {
        let mut tx = executor.begin().await?;
        let setup = query::init_queue_client_only(queue_name, false)?;
        for q in setup {
            sqlx::query(&q).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

impl<Args: Serialize + DeserializeOwned> PGMQueue<Args> {
    /// initialize a PGMQ connection with your own SQLx Postgres connection pool
    pub async fn new(pool: PgPool, queue_name: &str) -> Self {
        let config: Config<JsonCodec<Vec<u8>>> =
            Config::default().with_queue(queue_name.to_string());
        PGMQueue::new_with_config(pool, config).await
    }
}

impl<Args, C: Codec<Args, Compact = Vec<u8>>> PGMQueue<Args, C> {
    pub async fn new_with_config(pool: PgPool, config: Config<C>) -> Self {
        PGMQueue::create(config.queue(), &pool)
            .await
            .expect("Queue to be created");
        Self {
            sink: PgMqSink::new(pool.clone(), config.clone()),
            connection: pool,
            config,
            _args: PhantomData,
        }
    }

    async fn read_batch(
        config: Config<C>,
        connection: PgPool,
    ) -> Result<Option<Vec<Message>>, PgmqError> {
        let query = &query::read(
            config.queue(),
            config.visibility_timeout().as_secs() as i32,
            config.buffer_size() as i32,
        )?;
        let messages = fetch_messages(query, &connection).await?;
        Ok(messages)
    }
}

struct Message {
    msg_id: i64,
    visibility_time: DateTime<Utc>,
    read_count: i32,
    enqueued_at: DateTime<Utc>,
    message: Vec<u8>,
    headers: Value,
}

impl<Args, C> Backend for PGMQueue<Args, C>
where
    Args: Send + Sync + 'static + Unpin,
    C: Codec<Args, Compact = Vec<u8>> + Send + Sync + 'static,
    C::Error: std::error::Error + Send + Sync + 'static,
{
    type Args = Args;

    type Context = PgMqContext;

    type Beat = BoxStream<'static, Result<(), PgmqError>>;

    type Error = PgmqError;

    type IdType = i64;

    type Layer = AcknowledgeLayer<Self>;

    type Stream = TaskStream<PgMqTask<Args>, PgmqError>;

    fn heartbeat(&self, _worker: &WorkerContext) -> Self::Beat {
        Box::pin(stream::pending())
    }

    fn middleware(&self) -> Self::Layer {
        AcknowledgeLayer::new(self.clone())
    }

    fn poll(self, worker: &WorkerContext) -> Self::Stream {
        let ctx = PollContext::new(worker.clone(), Arc::default());
        let poller = self.config.poll_strategy().clone().build_stream(&ctx);
        stream::unfold(
            (self, poller, Vec::new()),
            |(backend, mut poller, mut buf)| async move {
                if let Some(msg) = buf.pop() {
                    return Some((Ok(msg), (backend, poller, buf)));
                }

                poller.next().await;

                match Self::read_batch(backend.config.clone(), backend.connection.clone()).await {
                    Ok(Some(messages)) => {
                        buf = messages;
                        buf.reverse();
                        let msg = buf.pop().unwrap();
                        Some((Ok(msg), (backend, poller, buf)))
                    }
                    Ok(None) => None,
                    Err(e) => Some((Err(e), (backend, poller, buf))),
                }
            },
        )
        .map(|res| match res {
            Ok(raw) => {
                let args =
                    C::decode(&raw.message).map_err(|e| PgmqError::ParsingError(e.into()))?;
                let ctx = PgMqContext {
                    enqueued_at: raw.enqueued_at,
                    headers: raw
                        .headers
                        .as_object()
                        .cloned()
                        .ok_or(PgmqError::ParsingError("Headers are not an object".into()))?,
                };
                let task = Task::builder(args)
                    .with_task_id(TaskId::new(raw.msg_id))
                    .with_attempt(Attempt::new_with_value(raw.read_count as usize))
                    .run_at_timestamp(raw.visibility_time.timestamp() as u64)
                    .with_ctx(ctx)
                    .build();
                Ok(Some(task))
            }
            Err(e) => Err(e),
        })
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, time::Duration};

    use apalis_core::{error::BoxDynError, worker::builder::WorkerBuilder};
    use futures::SinkExt;

    use super::*;

    #[tokio::test]
    async fn basic_worker() {
        let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
            .await
            .unwrap();

        PGMQueue::setup(&pool).await.unwrap();
        let mut backend = PGMQueue::new(pool, "basic_test").await;

        backend.send(Task::new(HashMap::new())).await.unwrap();

        async fn send_reminder(
            _: HashMap<String, String>,
            wrk: WorkerContext,
        ) -> Result<(), BoxDynError> {
            tokio::time::sleep(Duration::from_secs(2)).await;
            wrk.stop().unwrap();
            Ok(())
        }

        let worker = WorkerBuilder::new("rango-tango-1")
            .backend(backend)
            .build(send_reminder);
        worker.run().await.unwrap();
    }
}
