use std::{env, io, time::Duration};

use apalis::prelude::*;
use apalis_pgmq::*;
use facet::Facet;

#[derive(Debug, Clone, Default)]
struct FacetMsgPack;

impl<T: Facet<'static>> Codec<T> for FacetMsgPack {
    type Compact = Vec<u8>;
    type Error = io::Error;
    fn encode(&self, val: &T) -> Result<Self::Compact, Self::Error> {
        Ok(facet_msgpack::to_vec(val).unwrap())
    }

    fn decode(&self, val: &Self::Compact) -> Result<T, Self::Error> {
        Ok(facet_msgpack::from_slice(val).unwrap())
    }
}

#[derive(Facet)] // No need for serde
struct Reminder {
    to: String,
}

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();

    let config = Config::default()
        .queue("facet_msgpack")
        .track_worker(true)
        .heartbeat(Duration::from_secs(1))
        .store_results(true);

    let mut backend = PGMQueue::new(pool)
        .with_config(config)
        .with_codec(FacetMsgPack::default());

    backend
        .push(Reminder {
            to: "example@email.local".to_owned(),
        })
        .await
        .unwrap();

    async fn send_reminder(reminder: Reminder, wrk: WorkerContext) -> Result<(), BoxDynError> {
        println!("Sending reminder to {}", reminder.to);
        wrk.stop()?;
        Ok(())
    }

    let worker = WorkerBuilder::new("rango-tango-1")
        .backend(backend)
        .build(send_reminder);
    worker.run().await.unwrap();
}
