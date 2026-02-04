use std::{marker::PhantomData, time::Duration};

use apalis_codec::json::JsonCodec;
use apalis_core::backend::poll_strategy::{
    BackoffConfig, IntervalStrategy, MultiStrategy, StrategyBuilder,
};

/// Configuration for apalis-pgmq
#[derive(Debug)]
pub struct Config<Codec = JsonCodec<Vec<u8>>> {
    poll_strategy: MultiStrategy,
    buffer_size: usize,
    queue: String,
    visibility_timeout: Duration,
    _codec: PhantomData<Codec>,
}

impl<C> Clone for Config<C> {
    fn clone(&self) -> Self {
        Self {
            poll_strategy: self.poll_strategy.clone(),
            buffer_size: self.buffer_size,
            queue: self.queue.clone(),
            visibility_timeout: self.visibility_timeout,
            _codec: self._codec,
        }
    }
}

impl<Codec> Config<Codec> {
    /// Creates a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the queue name
    pub fn queue(&self) -> &str {
        &self.queue
    }

    /// Sets the queue name (builder style)
    pub fn with_queue(mut self, queue: String) -> Self {
        self.queue = queue;
        self
    }

    /// Gets the polling strategy
    pub fn poll_strategy(&self) -> &MultiStrategy {
        &self.poll_strategy
    }

    /// Sets the polling strategy (builder style)
    pub fn with_poll_strategy(mut self, strategy: MultiStrategy) -> Self {
        self.poll_strategy = strategy;
        self
    }

    /// Gets the buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Sets the buffer size (builder style)
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Gets the visibility timeout
    pub fn visibility_timeout(&self) -> Duration {
        self.visibility_timeout
    }

    /// Sets the visibility timeout (builder style)
    pub fn with_visibility_timeout(mut self, timeout: Duration) -> Self {
        self.visibility_timeout = timeout;
        self
    }
}

impl<C> Default for Config<C> {
    fn default() -> Self {
        let config = BackoffConfig::default().with_jitter(0.9);
        let interval = IntervalStrategy::new(Duration::from_millis(50)).with_backoff(config);
        let poll_strategy = StrategyBuilder::new().apply(interval).build();
        Self {
            poll_strategy,
            buffer_size: 100,
            queue: "default_queue".to_string(),
            visibility_timeout: Duration::from_secs(30),
            _codec: PhantomData,
        }
    }
}
