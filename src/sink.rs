use std::{
    collections::VecDeque,
    fmt::Debug,
    marker::PhantomData,
};
use pin_project::pin_project;
use crate::config::Config;

#[pin_project]
#[derive(Debug)]
pub(crate) struct PostgresSink<T, CompactType, C> {
    items: VecDeque<CompactType>,
    _phantom: PhantomData<(T, C)>,
}

impl<T, CompactType, C> Clone for PostgresSink<T, CompactType, C> {
    fn clone(&self) -> Self {
        Self {
            items: VecDeque::new(),
            _phantom: PhantomData,
        }
    }
}

impl<T, CompactType, C> PostgresSink<T, CompactType, C> {
    pub(crate) fn new(_pool: &sqlx::Pool<sqlx::Postgres>, _config: &Config) -> Self {
        Self {
            items: VecDeque::new(),
            _phantom: PhantomData,
        }
    }
}
