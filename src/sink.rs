use std::{
    collections::VecDeque,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use apalis_core::backend::future::BoxSyncFuture;
use futures::{FutureExt, Sink};
use pgmq::PgmqError;
use serde_json::Value;
use sqlx::PgPool;

use crate::{CompactType, PGMQ_SCHEMA, PGMQueue, PgMqTask, State, errors::Error};

pin_project_lite::pin_project! {
    pub struct PgMqSink<T> {
        items: VecDeque<PgMqTask>,
        pending_sends: VecDeque<BoxSyncFuture<Result<Vec<i64>, Error>>>,
        _marker: std::marker::PhantomData<T>,
    }
}

impl<T> Clone for PgMqSink<T> {
    fn clone(&self) -> Self {
        Self {
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
            _marker: PhantomData,
        }
    }
}

impl<T> PgMqSink<T> {
    pub(crate) fn new() -> Self {
        Self {
            items: VecDeque::new(),
            pending_sends: VecDeque::new(),
            _marker: std::marker::PhantomData,
        }
    }
}

struct MessageWithDelay {
    bytes: CompactType,
    delay: u64,
    headers: Option<serde_json::Value>,
}

impl<T> Sink<PgMqTask> for PGMQueue<T>
where
    T: Send + 'static + Unpin,
{
    type Error = Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = &mut self.get_mut();
        // 1. Make sure the queue exists before anything else happens.
        loop {
            match &mut this.state {
                State::Pre => {
                    let config = this.config.clone();
                    let pool = this.connection.clone();
                    this.state = State::Create(
                        async move { PGMQueue::create(&pool, &config).await }
                            .boxed()
                            .into(),
                    );
                }
                State::Create(fut) => match fut.poll_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(())) => {
                        this.state = State::Ready;
                        break;
                    }
                    Poll::Ready(Err(e)) => {
                        return Poll::Ready(Err(e));
                    }
                },
                _ => break,
            }
        }

        let sink = &mut this.sink;
        // Poll pending sends
        while let Some(pending) = sink.pending_sends.front_mut() {
            match pending.poll_unpin(cx) {
                Poll::Ready(Ok(_msg_ids)) => {
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

    fn start_send(self: Pin<&mut Self>, item: PgMqTask) -> Result<(), Self::Error> {
        let this = &mut self.get_mut().sink;

        this.items.push_back(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = &mut self.get_mut();
        let sink = &mut this.sink;

        let queue_name = this.config.queue.as_ref();

        // Collect all messages with their individual delays and headers
        let mut messages: Vec<MessageWithDelay> = Vec::new();

        while let Some(item) = sink.items.pop_front() {
            let delay = calculate_delay_seconds(item.run_at().unwrap_or_default() as i64);

            let headers = Some(serde_json::Value::Object(
                item.metadata()
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ));
            let bytes = item.args;

            messages.push(MessageWithDelay {
                bytes,
                delay,
                headers,
            });
        }

        // Create a single pending send for all messages
        if !messages.is_empty() {
            let conn = this.connection.clone();
            let queue_name = queue_name.to_string();

            let future = async move {
                let res = send_batch(&conn, &queue_name, &messages).await?;
                Ok(res)
            }
            .boxed()
            .into();

            sink.pending_sends.push_back(future);
        }

        // Now poll all pending sends
        while let Some(pending) = sink.pending_sends.front_mut() {
            match pending.poll_unpin(cx) {
                Poll::Ready(Ok(_)) => {
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

fn calculate_delay_seconds(run_at: i64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    run_at.saturating_sub(now) as u64
}

async fn send_batch(
    conn: &PgPool,
    queue_name: &str,
    messages: &[MessageWithDelay],
) -> Result<Vec<i64>, PgmqError> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }

    let mut groups: std::collections::BTreeMap<u64, Vec<&MessageWithDelay>> =
        std::collections::BTreeMap::new();

    for message in messages {
        groups.entry(message.delay).or_default().push(message);
    }

    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT ");

    for (i, (delay, messages)) in groups.iter().enumerate() {
        if i > 0 {
            query.push(" || ");
        }

        #[cfg(feature = "bytes-compat")]
        let compact_type = "::bytea[]";

        #[cfg(not(feature = "bytes-compat"))]
        let compact_type = "::jsonb[]";

        query.push(format!("{PGMQ_SCHEMA}.send_batch("));
        query.push("queue_name => ");
        query.push_bind(queue_name);
        query.push(", msgs => ");
        query.push_bind(messages.iter().map(|m| m.bytes.clone()).collect::<Vec<_>>());

        query.push(compact_type);
        query.push(", headers => ");
        query.push_bind(
            messages
                .iter()
                .map(|m| m.headers.clone())
                .collect::<Vec<_>>(),
        );
        query.push("::jsonb[]");
        query.push(", delay => ");
        query.push_bind(*delay as i32);
        query.push(")");
    }

    query.push(" AS msg_ids");

    let rows: Vec<(i64,)> = query.build_query_as().fetch_all(conn).await?;

    let msg_ids = rows.into_iter().map(|(id,)| id).collect::<Vec<_>>();

    Ok(msg_ids)
}
