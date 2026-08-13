use serde::de::DeserializeOwned;

use crate::{RedisClient, RedisClientError};

use super::{PubSubEnvelope, PubSubError};

pub struct RedisSubscriber {
    redis: RedisClient,
    receiver: tokio::sync::broadcast::Receiver<fred::types::Message>,
}

impl RedisSubscriber {
    pub async fn subscribe(redis: RedisClient, channel: &str) -> Result<Self, PubSubError> {
        let receiver = redis.message_rx();
        redis.subscribe(channel).await?;
        Ok(Self { redis, receiver })
    }

    pub async fn next<T: DeserializeOwned>(
        &mut self,
        expected_type: &str,
        supported_version: u16,
    ) -> Result<PubSubEnvelope<T>, PubSubError> {
        let message = self.receiver.recv().await.map_err(|error| match error {
            tokio::sync::broadcast::error::RecvError::Lagged(_) => PubSubError::Lagged,
            tokio::sync::broadcast::error::RecvError::Closed => PubSubError::Closed,
        })?;
        let payload = message
            .value
            .convert::<String>()
            .map_err(RedisClientError::Command)?;
        PubSubEnvelope::decode(&payload, expected_type, supported_version)
    }

    pub async fn close(self) -> Result<(), PubSubError> {
        self.redis.close().await.map_err(PubSubError::Redis)
    }
}
