use sqlx::{PgPool, Row, postgres::PgRow};

use crate::{Message, errors::PgmqError};

// Executes a query and returns multiple rows
// If the query returns no rows, None is returned
// This function is intended for internal use.
pub(crate) async fn fetch_messages(
    query: &str,
    connection: &PgPool,
) -> Result<Option<Vec<Message>>, PgmqError> {
    let mut messages: Vec<Message> = Vec::new();
    let result: Result<Vec<PgRow>, sqlx::Error> = sqlx::query(query).fetch_all(connection).await;
    match result {
        Ok(rows) => {
            if rows.is_empty() {
                Ok(None)
            } else {
                for row in rows.iter() {
                    let raw_msg = row.try_get("message")?;
                    messages.push(Message {
                        msg_id: row.try_get("msg_id")?,
                        visibility_time: row.try_get("vt")?,
                        read_count: row.try_get("read_ct")?,
                        enqueued_at: row.try_get("enqueued_at")?,
                        message: raw_msg,
                        headers: row.try_get("headers")?,
                    })
                }
                Ok(Some(messages))
            }
        }
        Err(e) => Err(e)?,
    }
}
