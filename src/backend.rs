use std::{fmt, marker::PhantomData};

use apalis_core::{
    backend::codec::json::JsonCodec,
    task::Task,
};
pub use apalis_sql::context::SqlContext;
pub use sqlx::{
    Pool, Postgres,
};
use ulid::Ulid;
use pin_project::pin_project;

use crate::{
    config::Config,
    sink::PostgresSink,
};

pub type PostgresTask<Args> = Task<Args, SqlContext, Ulid>;
pub type CompactType = Vec<u8>;

const JOBS_TABLE: &str = "jobs";

/// PostgresStorage is a storage backend for apalis using PostgreSQL as the database.
///
/// # Example
///
/// ```rust,no_run
/// use apalis_postgres::PostgresStorage;
/// use sqlx::PgPool;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let pool = PgPool::connect("postgres://postgres:postgres@localhost/postgres").await?;
///     PostgresStorage::setup(&pool).await?;
///     let storage = PostgresStorage::new(&pool);
///     Ok(())
/// }
/// ```
#[pin_project]
pub struct PostgresStorage<T, C> {
    pool: Pool<Postgres>,
    job_type: PhantomData<T>,
    config: Config,
    codec: PhantomData<C>,
    #[pin]
    sink: PostgresSink<T, CompactType, C>,
}

impl<T, C> fmt::Debug for PostgresStorage<T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresStorage")
            .field("pool", &self.pool)
            .field("job_type", &"PhantomData<T>")
            .field("config", &self.config)
            .field("codec", &std::any::type_name::<C>())
            .finish()
    }
}

impl<T, C: Clone> Clone for PostgresStorage<T, C> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            pool: self.pool.clone(),
            job_type: PhantomData,
            config: self.config.clone(),
            codec: self.codec,
        }
    }
}

impl PostgresStorage<(), ()> {
    #[cfg(feature = "migrate")]
    pub async fn setup(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
        Self::migrations().run(pool).await?;
        Ok(())
    }

    #[cfg(feature = "migrate")]
    #[must_use]
    pub fn migrations() -> sqlx::migrate::Migrator {
        sqlx::migrate!("./migrations")
    }
}

impl<T> PostgresStorage<T, ()> {
    #[must_use]
    pub fn new(pool: &Pool<Postgres>) -> PostgresStorage<T, JsonCodec<CompactType>> {
        let config = Config::new(std::any::type_name::<T>());
        PostgresStorage {
            pool: pool.clone(),
            job_type: PhantomData,
            sink: PostgresSink::new(pool, &config),
            config,
            codec: PhantomData,
        }
    }

    #[must_use]
    pub fn new_in_queue(
        pool: &Pool<Postgres>,
        queue: &str,
    ) -> PostgresStorage<T, JsonCodec<CompactType>> {
        let config = Config::new(queue);
        PostgresStorage {
            pool: pool.clone(),
            job_type: PhantomData,
            sink: PostgresSink::new(pool, &config),
            config,
            codec: PhantomData,
        }
    }

    #[must_use]
    pub fn new_with_config(
        pool: &Pool<Postgres>,
        config: &Config,
    ) -> PostgresStorage<T, JsonCodec<CompactType>> {
        PostgresStorage {
            pool: pool.clone(),
            job_type: PhantomData,
            config: config.clone(),
            codec: PhantomData,
            sink: PostgresSink::new(pool, config),
        }
    }
}

impl<T, C> PostgresStorage<T, C> {
    pub fn with_codec<NC>(self) -> PostgresStorage<T, NC> {
        let pool = self.pool.clone();
        let config = self.config.clone();
        PostgresStorage {
            pool: pool.clone(),
            job_type: PhantomData,
            config: config.clone(),
            codec: PhantomData,
            sink: PostgresSink::new(&pool, &config),
        }
    }

    pub fn pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}
