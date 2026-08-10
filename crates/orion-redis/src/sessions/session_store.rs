use std::time::{SystemTime, UNIX_EPOCH};

use orion_domain::UserId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{keys::RedisKey, RedisClient, RedisClientError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSession {
    pub id: Uuid,
    pub user_id: UserId,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Error)]
pub enum SessionStoreError {
    #[error("session Redis operation failed")]
    Redis(#[source] RedisClientError),
    #[error("session serialization failed")]
    Serialization(#[source] serde_json::Error),
    #[error("session has expired")]
    Expired,
}

#[derive(Clone)]
pub struct RedisSessionStore {
    redis: RedisClient,
}

impl RedisSessionStore {
    #[must_use]
    pub const fn new(redis: RedisClient) -> Self {
        Self { redis }
    }

    pub async fn issue(
        &self,
        user_id: UserId,
        ttl_seconds: u64,
    ) -> Result<RedisSession, SessionStoreError> {
        let now = epoch_seconds();
        let session = RedisSession {
            id: Uuid::new_v4(),
            user_id,
            issued_at: now,
            expires_at: now.saturating_add(ttl_seconds),
        };
        let payload = serde_json::to_string(&session).map_err(SessionStoreError::Serialization)?;
        let key = RedisKey::Session {
            session_id: session.id,
        }
        .to_string();
        self.redis
            .set_ex(key, payload, ttl_seconds.max(1) as i64)
            .await
            .map_err(SessionStoreError::Redis)?;
        Ok(session)
    }

    pub async fn load(&self, id: Uuid) -> Result<Option<RedisSession>, SessionStoreError> {
        let key = RedisKey::Session { session_id: id }.to_string();
        let Some(payload) = self
            .redis
            .get(key)
            .await
            .map_err(SessionStoreError::Redis)?
        else {
            return Ok(None);
        };
        let session = serde_json::from_str::<RedisSession>(&payload)
            .map_err(SessionStoreError::Serialization)?;
        if session.expires_at <= epoch_seconds() {
            self.revoke(id).await?;
            return Err(SessionStoreError::Expired);
        }
        Ok(Some(session))
    }

    pub async fn revoke(&self, id: Uuid) -> Result<(), SessionStoreError> {
        self.redis
            .delete(RedisKey::Session { session_id: id }.to_string())
            .await
            .map_err(SessionStoreError::Redis)
    }

    pub async fn ping(&self) -> Result<(), SessionStoreError> {
        self.redis.ping().await.map_err(SessionStoreError::Redis)
    }
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
