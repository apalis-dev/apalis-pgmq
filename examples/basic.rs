use apalis_core::backend::TaskSink;
use apalis_core::worker::builder::WorkerBuilder;
use apalis_core::Monitor;
use apalis_pgmq::{PgmqStorage, SqlContext};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EmailJob {
    to: String,
    subject: String,
    body: String,
}

async fn send_email(job: EmailJob, _ctx: SqlContext) {
    println!("Sending email to: {}", job.to);
    println!("Subject: {}", job.subject);
    println!("Body: {}", job.body);

    // Simulate email sending
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Email sent successfully!");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("Starting Apalis PGMQ Basic Example\n");

    // Database connection string
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    println!("Connecting to database: {}", db_url);

    // Create PGMQ storage backend
    let mut backend = PgmqStorage::<EmailJob>::new(&db_url, "email_queue").await?;

    println!("Backend created successfully\n");

    // Push some jobs to the queue
    println!("Pushing jobs to queue...");

    for i in 1..=5 {
        let job = EmailJob {
            to: format!("user{}@example.com", i),
            subject: format!("Test Email #{}", i),
            body: format!("This is test email number {}", i),
        };

        backend.push(job).await?;
        println!("   ✓ Pushed job #{}", i);
    }

    println!("\nStarting worker...\n");

    // Create and run worker
    let worker = WorkerBuilder::new("email-worker")
        .backend(backend)
        .build(send_email);

    // Run the monitor with a timeout for the example
    tokio::select! {
        result = Monitor::new().register(worker).run() => {
            result?;
        }
        _ = tokio::time::sleep(Duration::from_secs(30)) => {
            println!("\nExample timeout reached, shutting down...");
        }
    }

    println!("\nExample completed!");
    Ok(())
}
