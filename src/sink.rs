use std::{
    collections::VecDeque,
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use apalis_core::task::Task;
use futures::{FutureExt, Sink};
use serde::{Deserialize, Serialize};

use crate::{PgMq, PgMqContext, PgMqError};

#[derive(Debug)]
pub(super) struct PgMqSink<T> {
    items: VecDeque<Task<T, PgMqContext, i64>>,
    pending_sends: VecDeque<PendingSend>,
}

impl<T> Clone for PgMqSink<T> {
    fn clone(&self) -> Self {
        Self {
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
        }
    }
}

impl<T> PgMqSink<T> {
    pub(crate) fn new() -> Self {
        Self {
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
        }
    }
}

struct PendingSend {
    future: Pin<Box<dyn Future<Output = Result<i64, PgMqError>> + Send + 'static>>,
}

impl Debug for PendingSend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingSend").finish()
    }
}

impl<T> Sink<Task<T, PgMqContext, i64>> for PgMq<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + Unpin + 'static,
{
    type Error = PgMqError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let sink = self.get_mut().sink_mut();

        while let Some(pending) = sink.pending_sends.front_mut() {
            match pending.future.as_mut().poll(cx) {
                Poll::Ready(Ok(_msg_id)) => {
                    sink.pending_sends.pop_front();
                }
                Poll::Ready(Err(e)) => {
                    sink.pending_sends.pop_front();
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Task<T, PgMqContext, i64>) -> Result<(), Self::Error> {
        let sink = self.get_mut().sink_mut();
        sink.items.push_back(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();
        let queue = this.queue.clone();
        let queue_name = this.queue_name.clone();
        let sink = this.sink_mut();

        while let Some(item) = sink.items.pop_front() {
            let job = item.args;
            let queue = queue.clone();
            let queue_name = queue_name.clone();
            

            let future = async move {
                queue
                    .send(&queue_name, &job)
                    .await
                    .map_err(PgMqError::Pgmq)
            }
            .boxed();
            
            sink.pending_sends.push_back(PendingSend { future });
        }

        // Now poll all pending sends
        while let Some(pending) = sink.pending_sends.front_mut() {
            match pending.future.as_mut().poll(cx) {
                Poll::Ready(Ok(_msg_id)) => {
                    sink.pending_sends.pop_front();
                }
                Poll::Ready(Err(e)) => {
                    sink.pending_sends.pop_front();
                    return Poll::Ready(Err(e));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.poll_flush(cx)
    }
}