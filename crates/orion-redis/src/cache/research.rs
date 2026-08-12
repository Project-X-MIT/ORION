use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError, RedisKey};

/// Published research is disposable cache data. PostgreSQL remains the
/// authority and callers must invalidate this entry after a publication or
/// published-version change.
pub const RESEARCH_CACHE_TTL_SECONDS: i64 = 300;
pub const RESEARCH_CACHE_SCHEMA_VERSION: u16 = 1;

/// Durable policy events that make a published research cache entry unsafe to
/// serve. The event producer must commit its PostgreSQL policy change before
/// invoking the cache consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCacheInvalidationEvent {
    Publication,
    Withdrawal,
}

impl ResearchCacheInvalidationEvent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Publication => "publication",
            Self::Withdrawal => "withdrawal",
        }
    }
}

#[derive(Debug, Error)]
pub enum ResearchCacheError {
    #[error("research cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("research cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("research cache version cannot be empty")]
    EmptyVersion,
    #[error("research cache value is not published")]
    NotPublished,
    #[error("research cache value has no publication timestamp")]
    MissingPublicationTimestamp,
    #[error("research cache value version does not match its publication timestamp")]
    VersionMismatch,
    #[error("research cache schema version is unsupported")]
    UnsupportedSchemaVersion,
}

/// The cache envelope carries the published row version so an invalidation
/// producer can distinguish a stale write from the current cached value. `T`
/// must be a public published projection; this envelope is not a store for
/// drafts, review records/decisions, or Elo award state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedResearchCache<T> {
    pub schema_version: u16,
    pub published_version: String,
    pub value: T,
}

impl<T> PublishedResearchCache<T> {
    #[must_use]
    pub fn is_version(&self, version: &str) -> bool {
        self.published_version == version
    }
}

/// Reads one published research response from Redis.
pub async fn get_published<T>(
    redis: &RedisClient,
    research_id: Uuid,
) -> Result<Option<PublishedResearchCache<T>>, ResearchCacheError>
where
    T: DeserializeOwned,
{
    let key = RedisKey::Research { research_id }.to_string();
    let Some(payload) = redis.get(key).await? else {
        return Ok(None);
    };

    let entry = serde_json::from_str::<PublishedResearchCache<T>>(&payload)?;
    if entry.schema_version != RESEARCH_CACHE_SCHEMA_VERSION {
        return Err(ResearchCacheError::UnsupportedSchemaVersion);
    }
    if entry.published_version.trim().is_empty() {
        return Err(ResearchCacheError::EmptyVersion);
    }
    Ok(Some(entry))
}

/// Stores one already-published research response with its authoritative row
/// version. Drafts and private research must never call this function.
pub async fn set_published<T>(
    redis: &RedisClient,
    research_id: Uuid,
    published_version: &str,
    value: &T,
) -> Result<(), ResearchCacheError>
where
    T: Serialize,
{
    let published_version = published_version.trim();
    if published_version.is_empty() {
        return Err(ResearchCacheError::EmptyVersion);
    }

    let value = serde_json::to_value(value)?;
    validate_published_projection(&value, published_version)?;

    let entry = PublishedResearchCache {
        schema_version: RESEARCH_CACHE_SCHEMA_VERSION,
        published_version: published_version.to_owned(),
        value: &value,
    };
    let payload = serde_json::to_string(&entry)?;
    let key = RedisKey::Research { research_id }.to_string();
    redis
        .set_ex(key, payload, RESEARCH_CACHE_TTL_SECONDS)
        .await?;
    Ok(())
}

/// Removes a published research entry. Missing keys are treated as success.
pub async fn invalidate(redis: &RedisClient, research_id: Uuid) -> Result<(), ResearchCacheError> {
    let key = RedisKey::Research { research_id }.to_string();
    redis.delete(key).await?;
    Ok(())
}

/// Invalidates research after a committed publication or withdrawal policy
/// event. Redis deletion is intentionally idempotent so a retried event is
/// safe; PostgreSQL remains authoritative if Redis is unavailable.
pub async fn invalidate_after_policy_event(
    redis: &RedisClient,
    research_id: Uuid,
    event: ResearchCacheInvalidationEvent,
) -> Result<(), ResearchCacheError> {
    tracing::debug!(
        research_id = %research_id,
        event = event.as_str(),
        "invalidating research cache after policy event"
    );
    invalidate(redis, research_id).await
}

/// Invalidates after a committed publication event.
pub async fn invalidate_after_publication(
    redis: &RedisClient,
    research_id: Uuid,
) -> Result<(), ResearchCacheError> {
    invalidate_after_policy_event(
        redis,
        research_id,
        ResearchCacheInvalidationEvent::Publication,
    )
    .await
}

/// Invalidates after a committed withdrawal event.
pub async fn invalidate_after_withdrawal(
    redis: &RedisClient,
    research_id: Uuid,
) -> Result<(), ResearchCacheError> {
    invalidate_after_policy_event(
        redis,
        research_id,
        ResearchCacheInvalidationEvent::Withdrawal,
    )
    .await
}

/// Deletes the entry only when it still contains the version being replaced.
/// This prevents an older publication event from deleting a newer cache fill.
pub async fn invalidate_if_version(
    redis: &RedisClient,
    research_id: Uuid,
    published_version: &str,
) -> Result<bool, ResearchCacheError> {
    let key = RedisKey::Research { research_id }.to_string();
    let Some(payload) = redis.get(key.clone()).await? else {
        return Ok(false);
    };
    let entry = serde_json::from_str::<PublishedResearchCache<serde_json::Value>>(&payload)?;
    if entry.schema_version != RESEARCH_CACHE_SCHEMA_VERSION {
        return Err(ResearchCacheError::UnsupportedSchemaVersion);
    }
    if entry.published_version.trim().is_empty() {
        return Err(ResearchCacheError::EmptyVersion);
    }
    if !entry.is_version(published_version) {
        return Ok(false);
    }
    Ok(redis.delete_if_value(key, payload).await?)
}

fn validate_published_projection(
    value: &serde_json::Value,
    published_version: &str,
) -> Result<(), ResearchCacheError> {
    let Some(object) = value.as_object() else {
        return Err(ResearchCacheError::NotPublished);
    };
    if object.get("status").and_then(serde_json::Value::as_str) != Some("published") {
        return Err(ResearchCacheError::NotPublished);
    }
    let Some(value_version) = object
        .get("published_at")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return Err(ResearchCacheError::MissingPublicationTimestamp);
    };
    if value_version != published_version {
        return Err(ResearchCacheError::VersionMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        PublishedResearchCache, ResearchCacheInvalidationEvent, RESEARCH_CACHE_SCHEMA_VERSION,
    };

    #[test]
    fn policy_events_have_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&ResearchCacheInvalidationEvent::Publication).unwrap(),
            "\"publication\""
        );
        assert_eq!(
            serde_json::to_string(&ResearchCacheInvalidationEvent::Withdrawal).unwrap(),
            "\"withdrawal\""
        );
    }

    #[test]
    fn cache_envelope_preserves_the_published_version() {
        let entry = PublishedResearchCache {
            schema_version: RESEARCH_CACHE_SCHEMA_VERSION,
            published_version: "2026-08-12T10:00:00Z".to_owned(),
            value: json!({
                "status": "published",
                "published_at": "2026-08-12T10:00:00Z"
            }),
        };

        assert!(entry.is_version("2026-08-12T10:00:00Z"));
        assert!(!entry.is_version("2026-08-12T10:01:00Z"));
    }
}
