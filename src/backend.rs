use std::{fmt, marker::PhantomData, sync::Arc};

use apalis_core::{
    backend::{Backend, TaskSink, codec::json::JsonCodec},
    task::Task,
    worker::context::WorkerContext,
};
pub use apalis_sql::context::SqlContext;
use futures::{Stream, stream};
use pgmq::{PGMQueue, Message};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use ulid::Ulid;

use crate::{
    config::Config,
    sink::PgmqSink,
};

pub type PgmqTask<Args> = Task<Args, SqlContext, Ulid>;
pub type CompactType = Vec<u8>;

/// PgmqStorage is a storage backend for apalis using PGMQ (Postgres Message Queue).
///
/// # Example
///
/// ```rust,no_run
/// use apalis_pgmq::PgmqStorage;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let db_url = "postgres://postgres:postgres@localhost/postgres";
///     let storage = PgmqStorage::<MyJob>::new(db_url, "my_queue").await?;
///     Ok(())
/// }
/// ```
#[pin_project]
pub struct PgmqStorage<T, C> {
    queue: Arc<Mutex<PGMQueue>>,
    job_type: PhantomData<T>,
    config: Config,
    codec: PhantomData<C>,
    #[pin]
    sink: PgmqSink<T, CompactType, C>,
}

impl<T, C> fmt::Debug for PgmqStorage<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgmqStorage")
            .field("job_type", &"PhantomData<T>")
            .field("config", &self.config)
            .field("codec", &std::any::type_name::<C>())
            .finish()
    }
}

impl<T, C: Clone> Clone for PgmqStorage<T, C> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            queue: self.queue.clone(),
            job_type: PhantomData,
            config: self.config.clone(),
            codec: self.codec,
        }
    }
}

impl<T> PgmqStorage<T, ()> {
    /// Create a new PgmqStorage with the default JSON codec
    pub async fn new(
        db_url: impl Into<String>,
        queue_name: impl Into<String>,
    ) -> Result<PgmqStorage<T, JsonCodec<CompactType>>, crate::PgmqError> {
        let config = Config::new(queue_name);
        let queue = PGMQueue::new(db_url.into()).await?;
        
        // Create the queue if it doesn't exist
        queue.create(config.queue_name()).await?;
        
        let queue = Arc::new(Mutex::new(queue));
        
        Ok(PgmqStorage {
            queue: queue.clone(),
            job_type: PhantomData,
            sink: PgmqSink::new(queue, &config),
            config,
            codec: PhantomData,
        })
    }

    /// Create a new PgmqStorage with custom configuration
    pub async fn new_with_config(
        db_url: impl Into<String>,
        config: Config,
    ) -> Result<PgmqStorage<T, JsonCodec<CompactType>>, crate::PgmqError> {
        let queue = PGMQueue::new(db_url.into()).await?;
        
        // Create the queue if it doesn't exist
        queue.create(config.queue_name()).await?;
        
        let queue = Arc::new(Mutex::new(queue));
        
        Ok(PgmqStorage {
            queue: queue.clone(),
            job_type: PhantomData,
            config: config.clone(),
            codec: PhantomData,
            sink: PgmqSink::new(queue, &config),
        })
    }
}

impl<T, C> PgmqStorage<T, C> {
    pub fn with_codec<NC>(self) -> PgmqStorage<T, NC> {
        let queue = self.queue.clone();
        let config = self.config.clone();
        PgmqStorage {
            queue: queue.clone(),
            job_type: PhantomData,
            config: config.clone(),
            codec: PhantomData,
            sink: PgmqSink::new(queue, &config),
        }
    }

    pub fn queue(&self) -> Arc<Mutex<PGMQueue>> {
        self.queue.clone()
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl<T, C> Backend for PgmqStorage<T, C>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    type Args = T;
    type IdType = Ulid;
    type Context = SqlContext;
    type Error = crate::PgmqError;
    type Stream = std::pin::Pin<Box<dyn Stream<Item = Result<Option<Task<T, SqlContext, Ulid>>, crate::PgmqError>> + Send>>;
    type Beat = std::pin::Pin<Box<dyn Stream<Item = Result<(), crate::PgmqError>> + Send>>;
    type Layer = crate::LockTaskLayer;

    fn poll(self, _worker: &WorkerContext) -> Self::Stream {
        let queue = self.queue.clone();
        let queue_name = self.config.queue_name().to_string();
        let vt = self.config.visibility_timeout();

        Box::pin(async_stream::stream! {
            loop {
                let queue_guard = queue.lock().await;
                
                match queue_guard.read::<T>(&queue_name, Some(vt)).await {
                    Ok(Some(msg)) => {
                        match message_to_task(msg) {
                            Ok(task) => yield Ok(Some(task)),
                            Err(e) => yield Err(e),
                        }
                    }
                    Ok(None) => {
                        yield Ok(None);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        yield Err(crate::PgmqError::from(e));
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
                
                drop(queue_guard);
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        })
    }

    fn heartbeat(&self, _worker: &WorkerContext) -> Self::Beat {
        // PGMQ handles visibility timeout automatically, so we don't need explicit heartbeats
        Box::pin(stream::empty())
    }

    fn middleware(&self) -> Self::Layer {
        crate::LockTaskLayer::new()
    }
}


impl<T, C> TaskSink<T> for PgmqStorage<T, C>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    C: Clone + Send + Sync + 'static,
{
    async fn push(&mut self, job: T) -> Result<(), apalis_core::backend::TaskSinkError<crate::PgmqError>> {
        let queue_guard = self.queue.lock().await;
        queue_guard.send(self.config.queue_name(), &job)
            .await
            .map_err(|e| apalis_core::backend::TaskSinkError::PushError(crate::PgmqError::from(e)))?;
        Ok(())
    }

    async fn push_bulk(&mut self, jobs: Vec<T>) -> Result<(), apalis_core::backend::TaskSinkError<crate::PgmqError>> {
        for job in jobs {
            self.push(job).await?;
        }
        Ok(())
    }

    async fn push_stream(&mut self, mut jobs: impl Stream<Item = T> + Unpin + Send) -> Result<(), apalis_core::backend::TaskSinkError<crate::PgmqError>> {
        use futures::StreamExt;
        while let Some(job) = jobs.next().await {
            self.push(job).await?;
        }
        Ok(())
    }

    async fn push_task(&mut self, task: Task<T, SqlContext, Ulid>) -> Result<(), apalis_core::backend::TaskSinkError<crate::PgmqError>> {
        self.push(task.args).await
    }

    async fn push_all(&mut self, mut tasks: impl Stream<Item = Task<T, SqlContext, Ulid>> + Unpin + Send) -> Result<(), apalis_core::backend::TaskSinkError<crate::PgmqError>> {
        use futures::StreamExt;
        while let Some(task) = tasks.next().await {
            self.push(task.args).await?;
        }
        Ok(())
    }
}

fn message_to_task<T>(msg: Message<T>) -> Result<Task<T, SqlContext, Ulid>, crate::PgmqError>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    use apalis_core::task::task_id::TaskId;
    
    let task_id = Ulid::from_string(&msg.msg_id.to_string())
        .map_err(|e| crate::PgmqError::Other(format!("Invalid ULID: {}", e)))?;
    
    let ctx = SqlContext::default().with_max_attempts(3);
    
    use apalis_core::task::attempt::Attempt;
    
    let task = Task::builder(msg.message)
        .with_ctx(ctx)
        .with_task_id(TaskId::new(task_id))
        .with_attempt(Attempt::new_with_value(msg.read_ct as usize))
        .build();
    
    Ok(task)
}

