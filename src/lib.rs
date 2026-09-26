#![doc = include_str!("../README.md")]
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

use apalis_codec::json::JsonCodec;
use apalis_core::{
    backend::{
        Backend, BackendConfig, WireFormatBackend, finalize::Durable, future::BoxSyncFuture,
    },
    task::Task,
    timer::Delay,
    worker::{context::WorkerContext, ext::ack::AcknowledgeLayer},
};
use futures::{FutureExt, future::BoxFuture};
use pgmq::{PgmqError, util::CheckedName};
pub use sqlx::{PgPool, Postgres};
use tracing::{debug, info, trace};

use crate::config::StorageMode;
pub use crate::{config::Config, errors::Error, fetch::fetch_next, sink::PgMqSink};

mod ack;
mod config;
mod errors;
mod fetch;
pub mod query;
mod sink;

#[cfg(feature = "bytes-compat")]
pub const PGMQ_SCHEMA: &str = "apalis_pgmq";

#[cfg(not(feature = "bytes-compat"))]
pub const PGMQ_SCHEMA: &str = "pgmq";

pub const WORKERS_PREFIX: &str = r#"w"#;
pub const RESULTS_PREFIX: &str = r#"r"#;

#[cfg(feature = "bytes-compat")]
pub type CompactType = Vec<u8>;

#[cfg(not(feature = "bytes-compat"))]
pub type CompactType = serde_json::Value;

pub type PgMqTask<Args = CompactType> = Task<Args>;

pub struct PGMQueue<Args> {
    connection: PgPool,
    config: Config,
    sink: PgMqSink<Args>,
    codec: JsonCodec<CompactType>,
    state: State,
    heartbeat_timer: Option<Delay>,
}

impl<Args> Clone for PGMQueue<Args> {
    fn clone(&self) -> Self {
        Self {
            connection: self.connection.clone(),
            config: self.config.clone(),
            sink: self.sink.clone(),
            codec: self.codec.clone(),
            state: State::Pre,
            heartbeat_timer: None,
        }
    }
}

impl PGMQueue<()> {
    pub async fn setup(pool: &PgPool) -> Result<(), PgmqError> {
        let queue = pgmq::PGMQueueExt::new_with_pool(pool.clone()).await;
        queue.init_migrations_table("1.10.0").await?;

        // This allows us to use bytes instead of json
        #[cfg(feature = "bytes-compat")]
        sqlx::raw_sql(include_str!("../patches/json_to_bytes.sql"))
            .execute(pool)
            .await
            .map_err(|e| PgmqError::InstallationError(e.to_string()))?;
        Ok(())
    }
    pub async fn create(pool: &PgPool, config: &Config) -> Result<(), Error> {
        let mut tx = pool.begin().await?;
        let name = CheckedName::new(config.queue.as_ref()).unwrap();
        match &config.storage_mode {
            StorageMode::Default => {
                sqlx::query(&format!("SELECT {PGMQ_SCHEMA}.create($1)"))
                    .bind(name.as_ref())
                    .execute(&mut *tx)
                    .await?;
            }
            StorageMode::Unlogged => {
                sqlx::query(&format!("SELECT {PGMQ_SCHEMA}.create_unlogged($1)"))
                    .bind(name.as_ref())
                    .execute(&mut *tx)
                    .await?;
            }
            StorageMode::Partitioned {
                partition_interval,
                retention_interval,
            } => {
                sqlx::query(&format!(
                    "SELECT {PGMQ_SCHEMA}.create_partitioned($1, $2, $3)"
                ))
                .bind(name.as_ref())
                .bind(partition_interval)
                .bind(retention_interval)
                .execute(&mut *tx)
                .await?;
            }
        };
        if config.track_worker {
            let query = query::create_workers(name)?;
            sqlx::query(&query).execute(&mut *tx).await?;
        }

        if config.store_results {
            let query = query::create_results(name)?;
            sqlx::query(&query).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

impl<Args> PGMQueue<Args> {
    pub fn new(pool: PgPool) -> Self {
        let config: Config = Config::default();
        Self {
            sink: PgMqSink::new(),
            connection: pool,
            config,
            codec: JsonCodec::default(),
            state: State::Pre,
            heartbeat_timer: None,
        }
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    async fn read_batch(pool: PgPool, config: Config) -> Result<Vec<PgMqTask>, Error> {
        let tasks = fetch_next(&pool, &config).await?;
        Ok(tasks)
    }
    fn register_worker(
        &self,
        worker: &WorkerContext,
    ) -> Result<BoxFuture<'static, Result<(), Error>>, Error> {
        let query = query::register_worker(CheckedName::new(self.config.queue.as_ref())?)?;

        let pool = self.connection.clone();
        let worker = worker.clone();
        let heartbeat = self.config.heartbeat.as_secs();
        Ok(async move {
            let name = worker.name();
            let service = worker.get_service();
            sqlx::query(&query)
                .bind(name)
                .bind(service)
                .bind(heartbeat as i64)
                .execute(&pool)
                .await?;

            Ok(())
        }
        .boxed())
    }
}

enum State {
    Pre,
    Create(BoxSyncFuture<Result<(), Error>>),
    RegisterWorker(BoxSyncFuture<Result<(), Error>>),
    Ready,
    HeartBeat(BoxSyncFuture<Result<(), Error>>),
    Inflight(BoxSyncFuture<Result<Vec<PgMqTask>, Error>>),
    Buffering(VecDeque<PgMqTask>),
    CleanUp(BoxSyncFuture<Result<(), Error>>),
}

impl<Args> Backend for PGMQueue<Args>
where
    Args: Send + 'static,
{
    type Task = PgMqTask;
    type Error = Error;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
        worker: &WorkerContext,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            match &mut self.state {
                State::Pre => {
                    info!(
                        queue = %self.config.queue,
                        track_worker = self.config.track_worker,
                        "pgmq backend initializing"
                    );

                    let config = self.config.clone();
                    let pool = self.connection.clone();

                    self.state = State::Create(
                        async move { PGMQueue::create(&pool, &config).await }
                            .boxed()
                            .into(),
                    );

                    debug!("pgmq backend create future dispatched");
                }

                State::Create(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        trace!("pgmq backend creation pending");
                        return Poll::Pending;
                    }

                    Poll::Ready(Ok(())) => {
                        info!("pgmq backend initialized");

                        if !self.config.track_worker {
                            debug!("worker tracking disabled");

                            self.heartbeat_timer = Some(Delay::new(self.config.heartbeat));
                            self.state = State::Ready;
                            continue;
                        }

                        debug!(
                            worker = %worker.name(),
                            "registering worker"
                        );

                        self.state = State::RegisterWorker(self.register_worker(worker)?.into());
                    }

                    Poll::Ready(Err(e)) => {
                        info!(
                            error = ?e,
                            "pgmq backend initialization failed"
                        );

                        return Poll::Ready(Err(e));
                    }
                },

                State::RegisterWorker(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        trace!(
                            worker = %worker.name(),
                            "worker registration pending"
                        );

                        return Poll::Pending;
                    }

                    Poll::Ready(Ok(())) => {
                        info!(
                            worker = %worker.name(),
                            heartbeat = ?self.config.heartbeat,
                            "worker registered"
                        );

                        self.heartbeat_timer = Some(Delay::new(self.config.heartbeat));
                        self.state = State::Ready;

                        return Poll::Ready(Ok(()));
                    }

                    Poll::Ready(Err(e)) => {
                        info!(
                            worker = %worker.name(),
                            error = ?e,
                            "worker registration failed"
                        );

                        return Poll::Ready(Err(e));
                    }
                },

                State::HeartBeat(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        trace!(
                            worker = %worker.name(),
                            "worker heartbeat pending"
                        );

                        return Poll::Pending;
                    }

                    Poll::Ready(Ok(())) => {
                        debug!(
                            worker = %worker.name(),
                            "worker heartbeat completed"
                        );

                        self.heartbeat_timer = Some(Delay::new(self.config.heartbeat));
                        self.state = State::Ready;

                        return Poll::Ready(Ok(()));
                    }

                    Poll::Ready(Err(e)) => {
                        info!(
                            worker = %worker.name(),
                            error = ?e,
                            "worker heartbeat failed"
                        );

                        return Poll::Ready(Err(e));
                    }
                },

                State::Ready => {
                    if self.heartbeat_timer.is_none() {
                        if self.config.track_worker {
                            debug!(
                                worker = %worker.name(),
                                "worker heartbeat timer missing; re-registering worker"
                            );

                            self.state =
                                State::RegisterWorker(self.register_worker(worker)?.into());

                            continue;
                        }

                        trace!("initializing heartbeat timer");

                        self.heartbeat_timer = Some(Delay::new(self.config.heartbeat));
                    }

                    let heartbeat_due = Pin::new(self.heartbeat_timer.as_mut().unwrap())
                        .poll(cx)
                        .is_ready();

                    trace!(
                        heartbeat_due,
                        worker = %worker.name(),
                        "evaluating heartbeat timer"
                    );

                    if heartbeat_due {
                        debug!(
                            worker = %worker.name(),
                            queue = %self.config.queue,
                            "heartbeat due; dispatching heartbeat"
                        );

                        let pool = self.connection.clone();
                        let worker = worker.clone();

                        let query =
                            query::heartbeat_worker(CheckedName::new(self.config.queue.as_ref())?)?;

                        let fut = async move {
                            trace!(
                                worker = %worker.name(),
                                "executing worker heartbeat query"
                            );

                            sqlx::query(&query)
                                .bind(worker.name())
                                .execute(&pool)
                                .await?;

                            Ok(())
                        };

                        self.state = State::HeartBeat(fut.boxed().into());
                        continue;
                    }

                    trace!(
                        worker = %worker.name(),
                        "poll_ready: ready"
                    );

                    return Poll::Ready(Ok(()));
                }

                State::Inflight(_) => {
                    trace!("poll_ready: batch fetch in progress");
                    return Poll::Ready(Ok(()));
                }

                State::Buffering(buf) => {
                    trace!(buffered = buf.len(), "poll_ready: tasks buffered");

                    return Poll::Ready(Ok(()));
                }

                State::CleanUp(_) => {
                    unreachable!("cleanup during poll_ready")
                }
            }
        }
    }

    fn poll_next(
        &mut self,
        cx: &mut Context<'_>,
        _worker: &WorkerContext,
    ) -> Poll<Option<Result<Self::Task, Self::Error>>> {
        loop {
            match &mut self.state {
                State::Pre
                | State::Create(_)
                | State::CleanUp(_)
                | State::HeartBeat(_)
                | State::RegisterWorker(_) => {
                    unreachable!("poll_ready must have been called before poll_next")
                }

                State::Ready => {
                    debug!(
                        queue = %self.config.queue,
                        batch_size = self.config.batch_size,
                        "dispatching pgmq batch fetch"
                    );

                    let config = self.config.clone();
                    let pool = self.connection.clone();

                    self.state = State::Inflight(Self::read_batch(pool, config).boxed().into());
                }

                State::Buffering(buf) => {
                    if let Some(task) = buf.pop_front() {
                        trace!(remaining = buf.len(), "returning buffered pgmq task");

                        return Poll::Ready(Some(Ok(task)));
                    }

                    debug!("pgmq task buffer exhausted");
                    self.state = State::Ready;
                }

                State::Inflight(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        trace!("pgmq batch fetch pending");
                        return Poll::Pending;
                    }

                    Poll::Ready(Ok(res)) => {
                        debug!(fetched = res.len(), "pgmq batch fetch completed");

                        self.state = State::Buffering(VecDeque::from(res));
                        continue;
                    }

                    Poll::Ready(Err(e)) => {
                        info!(
                            error = ?e,
                            "pgmq batch fetch failed"
                        );

                        return Poll::Ready(Some(Err(e)));
                    }
                },
            }
        }
    }

    fn poll_close(
        &mut self,
        cx: &mut Context<'_>,
        worker: &WorkerContext,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            match &mut self.state {
                State::Pre | State::Create(_) | State::RegisterWorker(_) => {
                    unreachable!("poll_ready must have been called before poll_next")
                }

                State::Ready => {
                    info!(
                        worker = %worker.name(),
                        "pgmq backend closing"
                    );

                    return Poll::Ready(Ok(()));
                }

                State::Inflight(_) => {
                    debug!("pgmq backend closing while batch fetch is in progress");

                    return Poll::Ready(Ok(()));
                }

                State::Buffering(buf) => {
                    if buf.is_empty() {
                        debug!("pgmq task buffer empty; closing");

                        self.state = State::Ready;
                        continue;
                    }

                    let config = self.config.clone();
                    let pool = self.connection.clone();

                    let tasks: Vec<_> = std::mem::take(buf)
                        .iter()
                        .map(|s| s.task_id().unwrap().as_int().unwrap() as i64)
                        .collect();

                    debug!(
                        task_count = tasks.len(),
                        queue = %config.queue,
                        "releasing buffered pgmq tasks"
                    );

                    self.state = State::CleanUp(
                        async move {
                            trace!(
                                task_count = tasks.len(),
                                queue = %config.queue,
                                "executing pgmq visibility reset"
                            );

                            let query = format!("SELECT {PGMQ_SCHEMA}.set_vt($1, $2, 0);");

                            sqlx::query(&query)
                                .bind(config.queue.as_ref())
                                .bind(tasks)
                                .execute(&pool)
                                .await?;

                            debug!("pgmq visibility reset completed");

                            Ok(())
                        }
                        .boxed()
                        .into(),
                    );
                }

                State::HeartBeat(fut) | State::CleanUp(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => {
                        trace!("pgmq close operation pending");
                        return Poll::Pending;
                    }

                    Poll::Ready(Ok(_)) => {
                        debug!("pgmq close operation completed");

                        self.state = State::Ready;
                        continue;
                    }

                    Poll::Ready(Err(e)) => {
                        info!(
                            error = ?e,
                            "pgmq close operation failed"
                        );

                        return Poll::Ready(Err(e));
                    }
                },
            }
        }
    }
}

impl<Args> BackendConfig for PGMQueue<Args>
where
    Args: Send + Sync + 'static + Unpin,
{
    type Args = Args;

    type Id = u64;

    type Layer = AcknowledgeLayer<Self>;

    type Config = Config;

    type Kind = Durable;

    fn middleware(&mut self, _: &mut WorkerContext) -> Self::Layer {
        AcknowledgeLayer::new(self.clone())
    }

    fn config(&self) -> &Self::Config {
        &self.config
    }
}

impl<Args> WireFormatBackend for PGMQueue<Args>
where
    Args: Send + 'static + Unpin,
{
    type Compact = CompactType;

    type Codec = JsonCodec<CompactType>;
    fn codec(&self) -> &Self::Codec {
        &self.codec
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, time::Duration};

    use apalis::prelude::TaskSink;
    use apalis_core::{error::BoxDynError, worker::builder::WorkerBuilder};

    use super::*;

    #[tokio::test]
    async fn basic_worker() {
        let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
            .await
            .unwrap();

        PGMQueue::setup(&pool).await.unwrap();
        let config = Config::default().queue("basic_test");
        let mut backend = PGMQueue::new(pool).with_config(config);

        backend.push_task(Task::new(HashMap::new())).await.unwrap();

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
