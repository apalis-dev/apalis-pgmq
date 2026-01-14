use std::time::Duration;

/// Configuration for PGMQ backend.
#[derive(Debug, Clone)]
pub struct Config {
    /// The queue namespace/name
    namespace: String,
    /// Visibility timeout for messages
    visibility_timeout: Duration,
    /// Poll interval when queue is empty
    poll_interval: Duration,
    /// Maximum retry attempts before archiving
    max_retries: i32,
}

impl Config {
    /// Create a new configuration with the given namespace.
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            visibility_timeout: Duration::from_secs(30),
            poll_interval: Duration::from_millis(100),
            max_retries: 5,
        }
    }

    /// Set the visibility timeout for messages.
    pub fn with_visibility_timeout(mut self, timeout: Duration) -> Self {
        self.visibility_timeout = timeout;
        self
    }

    /// Set the poll interval for checking new messages.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set the maximum number of retry attempts.
    pub fn with_max_retries(mut self, max_retries: i32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Get the namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Get the visibility timeout.
    pub fn visibility_timeout(&self) -> Duration {
        self.visibility_timeout
    }

    /// Get the poll interval.
    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// Get the maximum retries.
    pub fn max_retries(&self) -> i32 {
        self.max_retries
    }
}
