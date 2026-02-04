use std::{any::type_name, convert::Infallible};

use apalis_core::{task::metadata::MetadataExt, task_fn::FromRequest};
use chrono::{DateTime, Utc};
use serde::{
    Deserialize, Serialize,
    de::{DeserializeOwned, Error},
};
use serde_json::Map;

use crate::PgMqTask;

/// Context for PgMq messages
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PgMqContext {
    pub headers: Map<String, serde_json::Value>,
    pub enqueued_at: DateTime<Utc>,
}

impl PgMqContext {
    /// Creates a new PgMqContext
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the headers
    pub fn headers(&self) -> &Map<String, serde_json::Value> {
        &self.headers
    }

    /// Sets the headers
    pub fn set_headers(&mut self, meta: Map<String, serde_json::Value>) {
        self.headers = meta;
    }
}

impl<Req: Sync> FromRequest<PgMqTask<Req>> for PgMqContext {
    type Error = Infallible;
    async fn from_request(req: &PgMqTask<Req>) -> Result<Self, Self::Error> {
        Ok(req.parts.ctx.clone())
    }
}

impl<T: Serialize + DeserializeOwned> MetadataExt<T> for PgMqContext {
    type Error = serde_json::Error;
    fn inject(&mut self, value: T) -> Result<(), Self::Error> {
        let json_value = serde_json::to_value(value)?;
        self.headers.insert(type_name::<T>().to_owned(), json_value);
        Ok(())
    }
    fn extract(&self) -> Result<T, Self::Error> {
        if let Some(value) = self.headers.get(type_name::<T>()) {
            let deserialized: T = T::deserialize(value)?;
            Ok(deserialized)
        } else {
            Err(serde_json::Error::custom(format!(
                "No metadata found for type: {}",
                type_name::<T>()
            )))
        }
    }
}
