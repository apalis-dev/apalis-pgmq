use std::fmt::Debug;

use apalis_core::{error::BoxDynError, task::Parts, worker::ext::ack::Acknowledge};
use futures::{
    FutureExt,
    future::{self, BoxFuture},
};

use crate::{PGMQueue, context::PgMqContext, errors::PgmqError, query};

impl<T, C, Res> Acknowledge<Res, PgMqContext, i64> for PGMQueue<T, C>
where
    T: Send,
    Res: Debug + Send + Sync,
    C: Send,
{
    type Error = PgmqError;

    type Future = BoxFuture<'static, Result<(), Self::Error>>;

    fn ack(
        &mut self,
        res: &Result<Res, BoxDynError>,
        parts: &Parts<PgMqContext, i64>,
    ) -> Self::Future {
        if res.is_ok() {
            let task_id = parts.task_id.as_ref().unwrap().inner().to_owned();
            let queue_name = self.config.queue().to_owned();
            let conn = self.connection.clone();

            let fut = async move {
                let query = query::archive_batch(&queue_name)?;
                let row = sqlx::query(&query).bind([task_id]).execute(&conn).await?;

                let num_archived = row.rows_affected();
                if num_archived == 1 {
                    Ok(())
                } else {
                    Err(PgmqError::DatabaseError(sqlx::Error::RowNotFound))
                }
            };
            return fut.boxed();
        }
        future::ready(Ok(())).boxed()
    }
}
