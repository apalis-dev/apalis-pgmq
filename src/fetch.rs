use std::collections::HashMap;

use apalis_core::task::{builder::TaskBuilder, metadata::MetadataStore, task_id::TaskId};
use pgmq::PgmqError;
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::{Config, PGMQ_SCHEMA, PgMqTask};

pub async fn fetch_next(connection: &PgPool, config: &Config) -> Result<Vec<PgMqTask>, PgmqError> {
    let query = format!(
        "SELECT msg_id, read_ct, enqueued_at, last_read_at, vt, message, headers FROM {PGMQ_SCHEMA}.read($1, $2, $3)"
    );
    let mut tasks = Vec::new();

    let result: Result<Vec<PgRow>, sqlx::Error> = sqlx::query(&query)
        .bind(config.queue.as_ref())
        .bind(config.visibility_timeout.as_secs() as i32)
        .bind(config.batch_size as i32)
        .fetch_all(connection)
        .await;
    match result {
        Ok(rows) => {
            for row in rows.iter() {
                let raw_msg = row.try_get("message")?;

                // visibility_time: row.try_get("vt")?,
                // enqueued_at: row.try_get("enqueued_at")?,

                let id: i64 = row.try_get("msg_id")?;
                let map: Value = row.try_get("headers")?;
                let map = match map {
                    Value::Object(inner) => inner
                        .into_iter()
                        .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_owned()))
                        .collect(),
                    _ => HashMap::new(),
                };
                let attempt: i32 = row.try_get("read_ct")?;
                let task = TaskBuilder::new(raw_msg)
                    .task_id(TaskId::from_int(id as u64))
                    .attempt(attempt as usize)
                    .with_metadata(MetadataStore::from_map(map))
                    .build();
                tasks.push(task);
            }
            Ok(tasks)
        }
        Err(e) => Err(e)?,
    }
}
