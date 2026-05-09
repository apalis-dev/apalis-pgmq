use std::{env, io};

use apalis::prelude::*;
use apalis_pgmq::*;
use facet::Facet;

struct FacetMsgPack;

impl<T: Facet<'static>> Codec<T> for FacetMsgPack {
    type Compact = Vec<u8>;
    type Error = io::Error;
    fn encode(val: &T) -> Result<Self::Compact, Self::Error> {
        Ok(facet_msgpack::to_vec(val).unwrap())
    }

    fn decode(val: &Self::Compact) -> Result<T, Self::Error> {
        Ok(facet_msgpack::from_slice(val).unwrap())
    }
}

#[derive(Facet)]
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
        .with_queue("facet_msgpack")
        .with_codec::<FacetMsgPack>();
    let mut backend = PGMQueue::new_with_config(pool, config).await;

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
