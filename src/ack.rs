use apalis_core::{
    error::BoxDynError,
    task::{ExecutionContext, status::Status},
    worker::ext::ack::Acknowledge,
};
use futures::{FutureExt, future::BoxFuture};
use pgmq::util::CheckedName;
use serde::Serialize;

use crate::{PGMQ_SCHEMA, PGMQueue, errors::Error, query};

impl<T, Res> Acknowledge<Res> for PGMQueue<T>
where
    T: Send,
    Res: Serialize + Send + Sync,
{
    type Error = Error;

    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn ack(&mut self, res: &Result<Res, BoxDynError>, parts: &ExecutionContext) -> Self::Future {
        let task_id = parts.task_id().unwrap().as_int().unwrap() as i64;
        let queue_name = self.config.queue.as_ref().to_owned();
        let conn = self.connection.clone();
        let status = parts.status();
        let config = self.config.clone();

        let mut result = None;

        if config.store_results {
            result = Some(serde_json::to_value(res.as_ref().map_err(|a| a.to_string())).unwrap())
        }

        let fut = async move {
            if status == Status::Killed || status == Status::Done {
                let query = format!("SELECT * from {PGMQ_SCHEMA}.archive($1, $2)");
                let row = sqlx::query(&query)
                    .bind(&queue_name)
                    .bind([task_id])
                    .execute(&conn)
                    .await
                    .map_err(|e| Error::Inner(e.into()))?;

                let num_archived = row.rows_affected();
                if num_archived != 1 {
                    return Err(Error::Inner(sqlx::Error::RowNotFound.into()));
                }
            }

            if config.store_results {
                let query = query::upsert_result(CheckedName::new(&queue_name)?)?;
                sqlx::query(&query)
                    .bind(task_id)
                    .bind(status.to_string())
                    .bind(result.unwrap())
                    .execute(&conn)
                    .await
                    .map_err(|e| Error::Inner(e.into()))?;
            }

            Ok(())
        };
        fut.boxed()
    }
}
