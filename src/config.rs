#[derive(Debug, Clone)]
pub struct Config {
    queue_name: String,
    buffer_size: usize,
}

impl Config {
    pub fn new(queue_name: impl Into<String>) -> Self {
        Self {
            queue_name: queue_name.into(),
            buffer_size: 10,
        }
    }

    pub fn set_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }
    
    pub fn queue_name(&self) -> &str {
        &self.queue_name
    }

    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }
}
