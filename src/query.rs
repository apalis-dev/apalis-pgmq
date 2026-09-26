use crate::errors::Error;
use crate::{PGMQ_SCHEMA, RESULTS_PREFIX, WORKERS_PREFIX};

use pgmq::util::CheckedName;

pub fn create_workers(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "
        CREATE TABLE IF NOT EXISTS {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name} (
            name TEXT PRIMARY KEY,
            service TEXT,
            started_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
            last_seen TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL
        );
        "
    ))
}

pub fn drop_workers(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "DROP TABLE IF EXISTS {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name};"
    ))
}

pub fn register_worker(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "

        INSERT INTO {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name} (name, service)
        VALUES ($1, $2)
        ON CONFLICT (name) DO UPDATE
        SET
            service = EXCLUDED.service,
            last_seen = NOW()
        WHERE {WORKERS_PREFIX}_{name}.last_seen < NOW() - ($3 * INTERVAL '1 second')
        RETURNING name
        "
    ))
}
pub fn deregister_worker(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "DELETE FROM {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name} WHERE worker_id = $1;"
    ))
}

pub fn heartbeat_worker(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "UPDATE {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name} SET last_seen = now() WHERE worker_id = $1;"
    ))
}

pub fn reap_stale_workers(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "DELETE FROM {PGMQ_SCHEMA}.{WORKERS_PREFIX}_{name} WHERE last_seen < now() - $1::interval;"
    ))
}

pub fn create_results(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "
        CREATE TABLE IF NOT EXISTS {PGMQ_SCHEMA}.{RESULTS_PREFIX}_{name} (
            msg_id BIGINT PRIMARY KEY,
            status TEXT NOT NULL,
            result JSONB,
            finished_at TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL
        );
        "
    ))
}

pub fn drop_results(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "DROP TABLE IF EXISTS {PGMQ_SCHEMA}.{RESULTS_PREFIX}_{name};"
    ))
}

pub fn upsert_result(name: CheckedName<'_>) -> Result<String, Error> {
    Ok(format!(
        "
        INSERT INTO {PGMQ_SCHEMA}.{RESULTS_PREFIX}_{name} (msg_id, status, result)
        VALUES ($1, $2, $3)
        ON CONFLICT (msg_id) DO UPDATE
        SET status = $2, result = $3, finished_at = now();
        "
    ))
}
