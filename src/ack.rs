use apalis_core::{
    error::{AbortError, BoxDynError},
    layers::{Layer, Service},
    task::{Parts, status::Status},
    worker::{context::WorkerContext, ext::ack::Acknowledge},
};
use apalis_sql::context::SqlContext;
use futures::{FutureExt, future::BoxFuture};
use pgmq::PGMQueue;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use ulid::Ulid;

use crate::PgmqTask;

#[derive(Clone, Debug)]
pub struct PgmqAck {
    queue: Arc<Mutex<PGMQueue>>,
    queue_name: String,
}

impl PgmqAck {
    pub fn new(queue: Arc<Mutex<PGMQueue>>, queue_name: String) -> Self {
        Self { queue, queue_name }
    }
}

impl<Res: Serialize + 'static> Acknowledge<Res, SqlContext, Ulid> for PgmqAck {
    type Error = crate::PgmqError;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;
    
    fn ack(
        &mut self,
        res: &Result<Res, BoxDynError>,
        parts: &Parts<SqlContext, Ulid>,
    ) -> Self::Future {
        let msg_id = parts.task_id;
        let status = calculate_status(parts, res);
        parts.status.store(status.clone());
        
        let queue = self.queue.clone();
        let queue_name = self.queue_name.clone();
        
        async move {
            let msg_id = msg_id
                .ok_or(crate::PgmqError::Other("TASK_ID_FOR_ACK not found".to_owned()))?
                .to_string()
                .parse::<i64>()
                .map_err(|e| crate::PgmqError::Other(format!("Invalid message ID: {}", e)))?;
            
            let queue_guard = queue.lock().await;
            
            match status {
                Status::Done => {
                    // Archive successful messages
                    queue_guard.archive(&queue_name, msg_id).await?;
                }
                Status::Killed => {
                    // Delete permanently failed messages
                    queue_guard.delete(&queue_name, msg_id).await?;
                }
                Status::Failed => {
                    // For retryable failures, do nothing - visibility timeout will expire
                    // and the message will become available again
                }
                _ => {}
            }
            
            Ok(())
        }
        .boxed()
    }
}

pub(crate) fn calculate_status<Res>(
    parts: &Parts<SqlContext, Ulid>,
    res: &Result<Res, BoxDynError>,
) -> Status {
    match &res {
        Ok(_) => Status::Done,
        Err(e) => match e {
            _ if parts.ctx.max_attempts() as usize <= parts.attempt.current() => Status::Killed,
            e if e.downcast_ref::<AbortError>().is_some() => Status::Killed,
            _ => Status::Failed,
        },
    }
}

// PGMQ handles locking via visibility timeout, so we don't need explicit lock operations
// The LockTaskLayer is kept for API compatibility but doesn't perform any locking

#[derive(Clone, Debug)]
pub struct LockTaskLayer;

impl LockTaskLayer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LockTaskLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for LockTaskLayer {
    type Service = LockTaskService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LockTaskService {
            inner,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LockTaskService<S> {
    inner: S,
}

impl<S, Args> Service<PgmqTask<Args>> for LockTaskService<S>
where
    S: Service<PgmqTask<Args>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<BoxDynError>,
    Args: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxDynError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(|e| e.into())
    }

    fn call(&mut self, mut req: PgmqTask<Args>) -> Self::Future {
        let worker_id = req
            .parts
            .data
            .get::<WorkerContext>()
            .map(|w| w.name().to_owned())
            .unwrap_or_else(|| "unknown".to_string());
        
        req.parts.ctx = req.parts.ctx.with_lock_by(Some(worker_id));
        let fut = self.inner.call(req);
        
        async move {
            // PGMQ handles locking automatically via visibility timeout
            fut.await.map_err(|e| e.into())
        }
        .boxed()
    }
}

