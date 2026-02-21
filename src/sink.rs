use std::{
    collections::VecDeque, pin::Pin, sync::Arc, task::{Context, Poll}
};

use apalis_core::backend::codec::Codec;
use chrono::Utc;
use futures::{
    FutureExt, Sink,
    future::{BoxFuture, Shared},
};
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::{PGMQueue, PgMqTask, config::Config, errors::PgmqError, query::enqueue_batch};

pin_project_lite::pin_project! {
    pub(super) struct PgMqSink<T, C> {
        conn: PgPool,
        config: Config<C>,
        items: VecDeque<PgMqTask<T>>,
        pending_sends: VecDeque<PendingSend>,
        _codec: std::marker::PhantomData<C>,
    }
}

impl<T, C> Clone for PgMqSink<T, C> {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            config: self.config.clone(),
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
            _codec: std::marker::PhantomData,
        }
    }
}

impl<T, C> PgMqSink<T, C> {
    pub(crate) fn new(conn: PgPool, config: Config<C>) -> Self {
        Self {
            conn,
            config,
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
            _codec: std::marker::PhantomData,
        }
    }
}

struct PendingSend {
    future: Shared<BoxFuture<'static, Result<Vec<i64>, Arc<PgmqError>>>>,
}

struct MessageWithDelay {
    bytes: Vec<u8>,
    delay: u64,
    headers: Option<serde_json::Value>,
}

impl<T, C> Sink<PgMqTask<T>> for PGMQueue<T, C>
where
    T: Send + 'static + Unpin,
    C: Codec<T, Compact = Vec<u8>> + Unpin,
    C::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = PgmqError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = &mut self.get_mut().sink;

        // Poll pending sends
        while let Some(pending) = this.pending_sends.front_mut() {
            match pending.future.poll_unpin(cx) {
                Poll::Ready(Ok(_msg_ids)) => {
                    this.pending_sends.pop_front();
                    println!("Completed pending send to PgMq");
                }
                Poll::Ready(Err(e)) => {
                    this.pending_sends.pop_front();
                    return Poll::Ready(Err(Arc::into_inner(e).unwrap()));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: PgMqTask<T>) -> Result<(), Self::Error> {
        let this = &mut self.get_mut().sink;

        this.items.push_back(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = &mut self.get_mut().sink;

        let queue_name = this.config.queue();

        // Collect all messages with their individual delays and headers
        let mut messages: Vec<MessageWithDelay> = Vec::new();

        while let Some(item) = this.items.pop_front() {
            let delay = calculate_delay_seconds(item.parts.run_at as i64);
            let bytes = C::encode(&item.args).map_err(|e| PgmqError::ParsingError(Box::new(e)))?;
            let headers = Some(serde_json::Value::Object(item.parts.ctx.headers));

            messages.push(MessageWithDelay {
                bytes,
                delay,
                headers,
            });
        }

        // Create a single pending send for all messages
        if !messages.is_empty() {
            let conn = this.conn.clone();
            let queue_name = queue_name.to_string();

            let future = async move { send_batch(&conn, &queue_name, &messages).await.map_err(Arc::new) }
                .boxed()
                .shared();

            this.pending_sends.push_back(PendingSend { future });
        }

        // Now poll all pending sends
        while let Some(pending) = this.pending_sends.front_mut() {
            match pending.future.poll_unpin(cx) {
                Poll::Ready(Ok(_)) => {
                    this.pending_sends.pop_front();
                }
                Poll::Ready(Err(e)) => {
                    this.pending_sends.pop_front();
                    return Poll::Ready(Err(Arc::into_inner(e).unwrap()));
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

fn calculate_delay_seconds(run_at: i64) -> u64 {
    let now = Utc::now().timestamp();
    if run_at > now {
        (run_at - now).max(0) as u64
    } else {
        0
    }
}

async fn send_batch(
    conn: &PgPool,
    queue_name: &str,
    messages: &[MessageWithDelay],
) -> Result<Vec<i64>, PgmqError> {
    let mut msg_ids: Vec<i64> = Vec::new();
    let query = enqueue_batch(queue_name, messages.len())?;

    let mut q = sqlx::query(&query);

    // Bind delays, messages, and headers in the correct order
    for msg in messages.iter() {
        q = q.bind(msg.delay as i64);
        q = q.bind(&msg.bytes);
        q = q.bind(&msg.headers);
    }

    let rows: Vec<PgRow> = q.fetch_all(conn).await?;
    for row in rows.iter() {
        msg_ids.push(row.get("msg_id"));
    }
    Ok(msg_ids)
}
