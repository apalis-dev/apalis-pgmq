use std::{env, time::Duration};

use apalis::prelude::*;
use apalis_pgmq::*;
use apalis_workflow::*;

#[tokio::main]
async fn main() {
    let pool = PgPool::connect(env::var("DATABASE_URL").unwrap().as_str())
        .await
        .unwrap();

    PGMQueue::setup(&pool).await.unwrap();
    let mut backend = PGMQueue::new(pool, "e_workflow").await;

    backend
        .push(serde_json::to_vec(&42u32).unwrap())
        .await
        .unwrap();

    async fn task1(task: u32) -> String {
        println!("Executing task1 with input: {}", task);
        (task + 99).to_string()
    }
    async fn task2(task: String) -> u32 {
        println!("Executing task2 with input: {}", task);
        task.parse::<u32>().unwrap() + 1
    }
    async fn task3(task: u32, worker: WorkerContext) {
        println!("Executing task3 with input: {}", task);
        assert_eq!(task, 142);
        worker.stop().unwrap();
    }
    let workflow = Workflow::new("test_workflow")
        .and_then(task1)
        .delay_for(Duration::from_secs(1))
        .and_then(task2)
        .and_then(task3);

    let worker = WorkerBuilder::new("rango-tango")
        .backend(backend)
        .on_event(|_c, e| {
            println!("{e:?},");
        })
        .build(workflow);
    worker.run().await.unwrap();
}
