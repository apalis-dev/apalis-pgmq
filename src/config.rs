use std::time::Duration;

use apalis_core::backend::queue::Queue;
use pgmq::util::check_input;

/// Configuration for a PGMQ-backed queue.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of messages fetched in a single batch.
    pub batch_size: usize,

    /// The PGMQ queue to use.
    pub queue: Queue,

    /// The default heartbeat to wake the worker
    ///
    /// Will call a keep alive if [`Self::track_worker`] is true
    pub heartbeat: Duration,

    /// How long a message remains invisible after being fetched.
    pub visibility_timeout: Duration,

    /// The storage mode used by the queue.
    pub storage_mode: StorageMode,

    /// Whether worker information should be tracked.
    pub track_worker: bool,

    /// Whether task results should be stored.
    pub store_results: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_secs(30),
            batch_size: 10,
            queue: Queue::from("default"),
            visibility_timeout: Duration::from_secs(30),
            storage_mode: StorageMode::Default,
            track_worker: false,
            store_results: false,
        }
    }
}

impl Config {
    /// Sets the maximum number of messages fetched in a single batch.
    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets the queue used by this configuration.
    pub fn queue(mut self, queue: impl AsRef<str>) -> Self {
        check_input(queue.as_ref()).expect("The queue name must be invalid");
        self.queue = queue.as_ref().into();
        self
    }

    /// Sets the heartbeat timeout.
    pub fn heartbeat(mut self, interval: Duration) -> Self {
        self.heartbeat = interval;
        self
    }

    /// Sets the message visibility timeout.
    pub fn visibility_timeout(mut self, timeout: Duration) -> Self {
        self.visibility_timeout = timeout;
        self
    }

    /// Sets the queue storage mode.
    pub fn mode(mut self, mode: StorageMode) -> Self {
        self.storage_mode = mode;
        self
    }

    /// Enables or disables worker tracking.
    pub fn track_worker(mut self, track_worker: bool) -> Self {
        self.track_worker = track_worker;
        self
    }

    /// Enables or disables result storage.
    pub fn store_results(mut self, store_results: bool) -> Self {
        self.store_results = store_results;
        self
    }
}

/// Storage mode used by the queue.
#[derive(Debug, Clone)]
pub enum StorageMode {
    /// Use the default queue layout.
    Default,

    /// Use an unlogged queue.
    Unlogged,

    /// Use a partitioned queue.
    Partitioned {
        /// How often a new partition is created.
        partition_interval: String,

        /// How long partitions are retained.
        retention_interval: String,
    },
}
