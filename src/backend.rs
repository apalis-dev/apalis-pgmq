use apalis_core::backend::Backend;
use apalis_core::task::Task;
use apalis_core::timer::sleep;
use apalis_core::worker::context::WorkerContext;
use futures::stream::{Stream, unfold};
use pgmq::{Message as PgmqMessage, PGMQueue};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::pin::Pin;

use crate::errors::PgMqError;
use crate::sink::PgMqSink;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PgMqContext {
    pub msg_id: Option<i64>,
    pub read_ct: Option<i32>,
    pub enqueued_at: Option<chrono::DateTime<chrono::Utc>>, 
    pub vt: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
pub struct PgMq<T> {
    pub(crate) queue: PGMQueue,
    pub(crate) queue_name: String,
    visibility_timeout: std::time::Duration,
    poll_interval: std::time::Duration,
    pub(crate) max_retries: i32,
    sink: PgMqSink<T>,
    _phantom: PhantomData<T>,
}

impl<T> PgMq<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    pub async fn new(connection_url: &str, queue_name: &str) -> Result<Self, PgMqError> {
        let queue = PGMQueue::new(connection_url.to_owned())
            .await
            .map_err(PgMqError::Pgmq)?;
        
        queue.create(queue_name)
            .await
            .map_err(PgMqError::Pgmq)?;
        
        Ok(Self {
            queue,
            queue_name: queue_name.to_owned(),
            visibility_timeout: std::time::Duration::from_secs(30),
            poll_interval: std::time::Duration::from_millis(100),
            max_retries: 5,
            sink: PgMqSink::new(),
            _phantom: PhantomData,
        })
    }

    pub fn with_visibility_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.visibility_timeout = timeout;
        self
    }

    pub fn with_poll_interval(mut self, interval: std::time::Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn with_max_retries(mut self, max_retries: i32) -> Self {
        self.max_retries = max_retries;
        self
    }
    
    pub async fn ack(&self, msg_id: i64) -> Result<(), PgMqError> {
        self.queue
            .delete(&self.queue_name, msg_id)
            .await
            .map_err(PgMqError::Pgmq)?;
        Ok(())
    }
    
    pub async fn archive(&self, msg_id: i64) -> Result<(), PgMqError> {
        self.queue
            .archive(&self.queue_name, msg_id)
            .await
            .map_err(PgMqError::Pgmq)?;
        Ok(())
    }

    pub async fn enqueue(&self, job: T) -> Result<i64, PgMqError> {
        let msg_id = self.queue
            .send(&self.queue_name, &job)
            .await
            .map_err(PgMqError::Pgmq)?;
        Ok(msg_id)
    }

    pub async fn retry(&self, msg_id: i64, read_count: i32) -> Result<(), PgMqError> {
        if read_count >= self.max_retries {
            self.archive(msg_id).await?;
        }
        Ok(())
    }

    pub(crate) fn sink_mut(&mut self) -> &mut PgMqSink<T> {
        &mut self.sink
    }
}

impl<T> Backend for PgMq<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + Unpin + 'static,
{
    type Args = T;
    type IdType = i64;
    type Context = PgMqContext;
    type Error = PgMqError;
    type Stream = Pin<Box<dyn Stream<Item = Result<Option<Task<Self::Args, Self::Context, Self::IdType>>, Self::Error>> + Send>>;
    type Beat = Pin<Box<dyn Stream<Item = Result<(), Self::Error>> + Send>>;
    type Layer = tower::layer::util::Identity;

    fn poll(self, _worker: &WorkerContext) -> Self::Stream {
        let queue = self.queue.clone();
        let queue_name = self.queue_name.clone();
        let visibility_timeout_secs = self.visibility_timeout.as_secs() as i32;
        let poll_interval = self.poll_interval;

        let state = (queue, queue_name, visibility_timeout_secs, poll_interval);

        Box::pin(unfold(state, |(queue, queue_name, visibility_timeout_secs, poll_interval)| async move {
            let result: Result<Option<PgmqMessage<T>>, _> = queue
                .read(&queue_name, Some(visibility_timeout_secs))
                .await;

            let next_state = (queue, queue_name, visibility_timeout_secs, poll_interval);

            match result {
                Ok(Some(msg)) => {
                    let context = PgMqContext {
                        msg_id: Some(msg.msg_id),
                        read_ct: Some(msg.read_ct),
                        enqueued_at: Some(msg.enqueued_at),
                        vt: Some(msg.vt),
                    };

                    let mut task = Task::new(msg.message);
                    task.parts.ctx = context;
                    task.parts.task_id = Some(apalis_core::task::task_id::TaskId::new(msg.msg_id));
                    
                    Some((Ok(Some(task)), next_state))
                }
                Ok(None) => {
                    sleep(poll_interval).await;
                    Some((Ok(None), next_state))
                }
                Err(e) => {
                    Some((Err(PgMqError::Pgmq(e)), next_state))
                }
            }
        }))
    }

    fn heartbeat(&self, _worker: &WorkerContext) -> Self::Beat {
        Box::pin(unfold((), |_| async {
            sleep(std::time::Duration::from_secs(30)).await;
            Some((Ok(()), ()))
        }))
    }

    fn middleware(&self) -> Self::Layer {
        tower::layer::util::Identity::new()
    }
}