use apalis_core::{
    error::{AbortError, BoxDynError},
    layers::{Layer, Service},
    task::{Parts, status::Status},
    worker::{context::WorkerContext, ext::ack::Acknowledge},
};
use apalis_sql::context::SqlContext;
use futures::{FutureExt, future::BoxFuture};
use serde::Serialize;
use sqlx::PgPool;
use ulid::Ulid;

use crate::PostgresTask;

#[derive(Clone, Debug)]
pub struct PostgresAck {
    pool: PgPool,
}

impl PostgresAck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl<Res: Serialize + 'static> Acknowledge<Res, SqlContext, Ulid> for PostgresAck {
    type Error = sqlx::Error;
    type Future = BoxFuture<'static, Result<(), Self::Error>>;
    
    fn ack(
        &mut self,
        res: &Result<Res, BoxDynError>,
        parts: &Parts<SqlContext, Ulid>,
    ) -> Self::Future {
        let task_id = parts.task_id;
        let worker_id = parts.ctx.lock_by().clone();

        let response = serde_json::to_string(&res.as_ref().map_err(|e| e.to_string()));

        let status = calculate_status(parts, res);
        parts.status.store(status.clone());
        let attempt = parts.attempt.current() as i32;
        let pool = self.pool.clone();
        let res = response.map_err(|e| sqlx::Error::Decode(e.into()));
        let status_str = status.to_string();
        
        async move {
            let task_id = task_id
                .ok_or(sqlx::Error::ColumnNotFound("TASK_ID_FOR_ACK".to_owned()))?
                .to_string();
            let worker_id =
                worker_id.ok_or(sqlx::Error::ColumnNotFound("WORKER_ID_LOCK_BY".to_owned()))?;
            let res_ok = res?;
            
            let query = r#"
                UPDATE jobs
                SET status = $1,
                    attempts = $2,
                    last_error = $3,
                    done_at = CASE WHEN $1 = 'Done' THEN NOW() ELSE NULL END,
                    updated_at = NOW()
                WHERE id::text = $4
                  AND lock_by = $5
            "#;
            
            let result = sqlx::query(query)
                .bind(&status_str)
                .bind(attempt)
                .bind(&res_ok)
                .bind(&task_id)
                .bind(&worker_id)
                .execute(&pool)
                .await?;

            if result.rows_affected() == 0 {
                return Err(sqlx::Error::RowNotFound);
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

pub(crate) async fn lock_task(
    pool: &PgPool,
    task_id: &Ulid,
    worker_id: &str,
) -> Result<(), sqlx::Error> {
    let task_id = task_id.to_string();
    
    let query = r#"
        UPDATE jobs
        SET lock_at = NOW(),
            lock_by = $1,
            updated_at = NOW()
        WHERE id::text = $2
          AND (lock_at IS NULL OR lock_at < NOW() - INTERVAL '5 minutes')
    "#;
    
    let res = sqlx::query(query)
        .bind(worker_id)
        .bind(&task_id)
        .execute(pool)
        .await?;

    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct LockTaskLayer {
    pool: PgPool,
}

impl LockTaskLayer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl<S> Layer<S> for LockTaskLayer {
    type Service = LockTaskService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LockTaskService {
            inner,
            pool: self.pool.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LockTaskService<S> {
    inner: S,
    pool: PgPool,
}

impl<S, Args> Service<PostgresTask<Args>> for LockTaskService<S>
where
    S: Service<PostgresTask<Args>> + Send + 'static,
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

    fn call(&mut self, mut req: PostgresTask<Args>) -> Self::Future {
        let pool = self.pool.clone();
        let worker_id = req
            .parts
            .data
            .get::<WorkerContext>()
            .map(|w| w.name().to_owned())
            .unwrap();
        let parts = &req.parts;
        let task_id = match &parts.task_id {
            Some(id) => *id.inner(),
            None => {
                return async {
                    Err(sqlx::Error::ColumnNotFound("TASK_ID_FOR_LOCK".to_owned()).into())
                }
                .boxed();
            }
        };
        req.parts.ctx = req.parts.ctx.with_lock_by(Some(worker_id.clone()));
        let fut = self.inner.call(req);
        async move {
            lock_task(&pool, &task_id, &worker_id).await?;

            fut.await.map_err(|e| e.into())
        }
        .boxed()
    }
}
