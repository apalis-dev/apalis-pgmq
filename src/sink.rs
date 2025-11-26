use std::{
    collections::VecDeque,
    fmt::Debug,
    marker::PhantomData,
    sync::Arc,
};
use pin_project::pin_project;
use pgmq::PGMQueue;
use tokio::sync::Mutex;

use crate::config::Config;

#[pin_project]
#[derive(Debug)]
pub(crate) struct PgmqSink<T, CompactType, C> {
    queue: Arc<Mutex<PGMQueue>>,
    config: Config,
    items: VecDeque<CompactType>,
    _phantom: PhantomData<(T, C)>,
}

impl<T, CompactType, C> Clone for PgmqSink<T, CompactType, C> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            config: self.config.clone(),
            items: VecDeque::new(),
            _phantom: PhantomData,
        }
    }
}

impl<T, CompactType, C> PgmqSink<T, CompactType, C> {
    pub(crate) fn new(queue: Arc<Mutex<PGMQueue>>, config: &Config) -> Self {
        Self {
            queue,
            config: config.clone(),
            items: VecDeque::new(),
            _phantom: PhantomData,
        }
    }
}

