#[derive(Debug, Clone)]
pub struct Config {
    queue_name: String,
    buffer_size: usize,
    visibility_timeout: i32,
}

impl Config {
    pub fn new(queue_name: impl Into<String>) -> Self {
        Self {
            queue_name: queue_name.into(),
            buffer_size: 10,
            visibility_timeout: 30, // 30 seconds default
        }
    }

    pub fn set_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    pub fn set_visibility_timeout(mut self, timeout: i32) -> Self {
        self.visibility_timeout = timeout;
        self
    }
    
    pub fn queue_name(&self) -> &str {
        &self.queue_name
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    pub fn visibility_timeout(&self) -> i32 {
        self.visibility_timeout
    }
}
