use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PubSubEnvelope<T> {
    pub event_id: Uuid,
    pub event_type: String,
    pub schema_version: u16,
    pub payload: T,
}

impl<T> PubSubEnvelope<T> {
    pub fn validate(&self, expected_type: &str, supported_version: u16) -> Result<(), PubSubError> {
        if self.event_type != expected_type {
            return Err(PubSubError::UnexpectedType);
        }
        if self.schema_version != supported_version {
            return Err(PubSubError::IncompatibleVersion);
        }
        Ok(())
    }
}

impl<T: DeserializeOwned> PubSubEnvelope<T> {
    pub fn decode(
        payload: &str,
        expected_type: &str,
        supported_version: u16,
    ) -> Result<Self, PubSubError> {
        let envelope: Self = serde_json::from_str(payload).map_err(PubSubError::Serialization)?;
        envelope.validate(expected_type, supported_version)?;
        Ok(envelope)
    }
}

#[derive(Debug, Error)]
pub enum PubSubError {
    #[error("Redis Pub/Sub operation failed")]
    Redis(#[from] RedisClientError),
    #[error("Pub/Sub envelope serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("unexpected Pub/Sub event type")]
    UnexpectedType,
    #[error("incompatible Pub/Sub event version")]
    IncompatibleVersion,
    #[error("Pub/Sub receiver lagged")]
    Lagged,
    #[error("Pub/Sub receiver closed")]
    Closed,
}

#[derive(Clone)]
pub struct RedisPublisher {
    redis: RedisClient,
}

impl RedisPublisher {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self { redis }
    }

    pub async fn publish<T: Serialize>(
        &self,
        channel: &str,
        envelope: &PubSubEnvelope<T>,
    ) -> Result<u64, PubSubError> {
        envelope.validate(&envelope.event_type, envelope.schema_version)?;
        let payload = serde_json::to_string(envelope)?;
        let receivers = self.redis.publish(channel, payload).await?;
        Ok(u64::try_from(receivers).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::{PubSubEnvelope, PubSubError};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn incompatible_versions_are_rejected() {
        let raw = serde_json::json!({
            "event_id": Uuid::nil(), "event_type": "orion.notification.requested",
            "schema_version": 2, "payload": {"message": "safe"}
        })
        .to_string();
        let result =
            PubSubEnvelope::<serde_json::Value>::decode(&raw, "orion.notification.requested", 1);
        assert!(matches!(result, Err(PubSubError::IncompatibleVersion)));
    }

    #[test]
    fn envelope_contains_only_explicit_contract_fields() {
        let envelope = PubSubEnvelope {
            event_id: Uuid::nil(),
            event_type: "orion.rating.updated".into(),
            schema_version: 1,
            payload: json!({"rating": 1000}),
        };
        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert!(value.get("token").is_none());
        assert!(value.get("credentials").is_none());
    }
}
