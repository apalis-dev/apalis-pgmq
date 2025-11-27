use apalis_core::backend::TaskSink;
use apalis_core::error::BoxDynError;
use apalis_core::worker::builder::WorkerBuilder;
use apalis_core::Monitor;
use apalis_pgmq::{PgmqStorage, SqlContext};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessDataJob {
    id: u32,
    data: String,
    should_fail: bool,
}

async fn process_data(job: ProcessDataJob, _ctx: SqlContext) -> Result<(), BoxDynError> {
    println!("Processing job #{}: {}", job.id, job.data);

    // Simulate processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    if job.should_fail {
        println!("Job #{} failed! Will retry...", job.id);
        return Err("Simulated failure".into());
    }

    println!("Job #{} completed successfully!", job.id);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("Starting Apalis PGMQ Retry Example\n");

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    println!("Connecting to database: {}", db_url);

    // Create PGMQ storage backend
    let mut backend = PgmqStorage::<ProcessDataJob>::new(&db_url, "retry_queue").await?;

    println!("Backend created for retry_queue\n");

    // Push jobs - some will succeed, some will fail
    println!("Pushing jobs to queue...");

    backend
        .push(ProcessDataJob {
            id: 1,
            data: "Success job".to_string(),
            should_fail: false,
        })
        .await?;
    println!("   ✓ Pushed job #1 (will succeed)");

    backend
        .push(ProcessDataJob {
            id: 2,
            data: "Failure job".to_string(),
            should_fail: true,
        })
        .await?;
    println!("   ✓ Pushed job #2 (will fail and retry)");

    backend
        .push(ProcessDataJob {
            id: 3,
            data: "Another success".to_string(),
            should_fail: false,
        })
        .await?;
    println!(" Pushed job #3 (will succeed)");

    println!("\nStarting worker...");
    println!("Failed jobs will be retried according to backend configuration\n");

    // Create and run worker
    let worker = WorkerBuilder::new("retry-worker")
        .backend(backend)
        .build(process_data);

    // Run with timeout
    tokio::select! {
        result = Monitor::new().register(worker).run() => {
            result?;
        }
        _ = tokio::time::sleep(Duration::from_secs(60)) => {
            println!("\nExample timeout reached, shutting down...");
        }   
    }

    println!("\nExample completed!");

    Ok(())
}
