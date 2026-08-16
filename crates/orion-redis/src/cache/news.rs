use chrono::{DateTime, Duration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::{redis_key, RedisClient, RedisClientError, RedisKey};

const NEWS_FEED_CACHE_KEY_ID: &str = "cache.news_feed";
/// News feed cache entries are disposable and may never be served after this
/// freshness budget, even if Redis has retained the value unexpectedly.
pub const NEWS_FEED_CACHE_TTL_SECONDS: i64 = 120;
pub const NEWS_FEED_CACHE_SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Error)]
pub enum NewsCacheError {
    #[error("news cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("news cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("news feed cache key is not registered")]
    UnregisteredKey,
    #[error("news cache schema version is unsupported")]
    UnsupportedSchemaVersion,
}

/// Cache envelope for a public, already-sanitized feed response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsFeedCache<T> {
    pub schema_version: u16,
    pub cached_at: DateTime<Utc>,
    pub value: T,
}

impl<T> NewsFeedCache<T> {
    #[must_use]
    pub fn is_fresh_at(&self, now: DateTime<Utc>) -> bool {
        let age = now.signed_duration_since(self.cached_at);
        age >= Duration::zero() && age < Duration::seconds(NEWS_FEED_CACHE_TTL_SECONDS)
    }
}

/// Returns a fresh page from Redis. A stale or future-dated value is treated
/// as a miss; PostgreSQL remains authoritative and will be queried by the API.
pub async fn get<T>(
    redis: &RedisClient,
    limit: u32,
    offset: u64,
) -> Result<Option<NewsFeedCache<T>>, NewsCacheError>
where
    T: DeserializeOwned,
{
    let key = RedisKey::NewsFeed { limit, offset }.to_string();
    let Some(payload) = redis.get(key).await? else {
        return Ok(None);
    };

    let entry = decode_entry::<T>(&payload)?;
    if entry.is_fresh_at(Utc::now()) {
        Ok(Some(entry))
    } else {
        Ok(None)
    }
}

/// Stores one sanitized public feed page. Redis TTL is bounded by the same
/// freshness budget checked by [`get`], so cache availability cannot extend
/// the allowed staleness window.
pub async fn set<T>(
    redis: &RedisClient,
    limit: u32,
    offset: u64,
    value: &T,
) -> Result<(), NewsCacheError>
where
    T: Serialize,
{
    let entry = NewsFeedCache {
        schema_version: NEWS_FEED_CACHE_SCHEMA_VERSION,
        cached_at: Utc::now(),
        value,
    };
    let payload = serde_json::to_string(&entry)?;
    let key = RedisKey::NewsFeed { limit, offset }.to_string();
    redis
        .set_ex(key, payload, NEWS_FEED_CACHE_TTL_SECONDS)
        .await?;
    Ok(())
}

/// Invalidates every registered news-feed page after a committed ingestion
/// event. PostgreSQL remains authoritative if Redis is unavailable.
pub async fn invalidate_after_ingestion(redis: &RedisClient) -> Result<u64, NewsCacheError> {
    let spec = redis_key(NEWS_FEED_CACHE_KEY_ID).ok_or(NewsCacheError::UnregisteredKey)?;
    let pattern = spec
        .pattern
        .replace("{limit}", "*")
        .replace("{offset}", "*");
    Ok(redis.delete_pattern(pattern).await?)
}

fn decode_entry<T>(payload: &str) -> Result<NewsFeedCache<T>, NewsCacheError>
where
    T: DeserializeOwned,
{
    let entry = serde_json::from_str::<NewsFeedCache<T>>(payload)?;
    if entry.schema_version != NEWS_FEED_CACHE_SCHEMA_VERSION {
        return Err(NewsCacheError::UnsupportedSchemaVersion);
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use serde_json::json;

    use crate::{redis_key, RedisKey, RedisTtl};

    use super::{
        decode_entry, NewsCacheError, NewsFeedCache, NEWS_FEED_CACHE_SCHEMA_VERSION,
        NEWS_FEED_CACHE_TTL_SECONDS,
    };

    #[test]
    fn cache_freshness_never_exceeds_the_budget() {
        let cached_at = Utc.with_ymd_and_hms(2026, 8, 14, 10, 0, 0).unwrap();
        let entry = NewsFeedCache {
            schema_version: NEWS_FEED_CACHE_SCHEMA_VERSION,
            cached_at,
            value: json!({"items": []}),
        };

        assert!(entry.is_fresh_at(cached_at + Duration::seconds(NEWS_FEED_CACHE_TTL_SECONDS - 1)));
        assert!(!entry.is_fresh_at(cached_at + Duration::seconds(NEWS_FEED_CACHE_TTL_SECONDS)));
        assert!(!entry.is_fresh_at(cached_at - Duration::seconds(1)));
    }

    #[test]
    fn recent_feed_cache_matches_the_registered_key_and_ttl() {
        let spec = redis_key("cache.news_feed").expect("news feed cache key is registered");
        assert_eq!(spec.pattern, "orion:v1:cache:news_feed:{limit}:{offset}");
        assert_eq!(
            spec.ttl,
            RedisTtl::Seconds(
                u64::try_from(NEWS_FEED_CACHE_TTL_SECONDS).expect("cache TTL is non-negative")
            )
        );
        assert_eq!(
            RedisKey::NewsFeed {
                limit: 20,
                offset: 0
            }
            .to_string(),
            "orion:v1:cache:news_feed:20:0"
        );
    }

    #[test]
    fn cache_rejects_an_unknown_schema_version() {
        let payload = serde_json::json!({
            "schema_version": NEWS_FEED_CACHE_SCHEMA_VERSION + 1,
            "cached_at": "2026-08-14T10:00:00Z",
            "value": {"items": []}
        });

        let result = decode_entry::<serde_json::Value>(&payload.to_string());
        assert!(matches!(
            result,
            Err(NewsCacheError::UnsupportedSchemaVersion)
        ));
    }
}
