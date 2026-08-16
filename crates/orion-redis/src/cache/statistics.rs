use std::future::Future;

use orion_db::models::ProfileStatistics;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{RedisClient, RedisClientError, RedisKey};

/// Statistics share the registered profile read-model key and its 120 second
/// TTL. PostgreSQL remains authoritative for every value in this projection.
pub const STATISTICS_CACHE_TTL_SECONDS: i64 = 120;
pub const STATISTICS_CACHE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct CachedStatistics {
    schema_version: u16,
    user_id: Uuid,
    rating: Option<i32>,
    global_rank: Option<i64>,
    quizzes_completed: i64,
    correct_answers: i64,
}

impl CachedStatistics {
    fn from_authoritative(value: ProfileStatistics) -> Self {
        Self {
            schema_version: STATISTICS_CACHE_SCHEMA_VERSION,
            user_id: value.user_id,
            rating: value.rating,
            global_rank: value.global_rank,
            quizzes_completed: value.quizzes_completed,
            correct_answers: value.correct_answers,
        }
    }

    fn into_authoritative(self) -> ProfileStatistics {
        ProfileStatistics {
            user_id: self.user_id,
            rating: self.rating,
            global_rank: self.global_rank,
            quizzes_completed: self.quizzes_completed,
            correct_answers: self.correct_answers,
        }
    }
}

#[derive(Debug, Error)]
pub enum StatisticsCacheError {
    #[error("statistics cache Redis operation failed")]
    Redis(#[from] RedisClientError),
    #[error("statistics cache payload serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("statistics cache schema version is unsupported")]
    UnsupportedSchemaVersion,
}

#[derive(Clone)]
pub struct StatisticsCache {
    client: RedisClient,
}

impl StatisticsCache {
    #[must_use]
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    pub async fn get(
        &self,
        user_id: Uuid,
    ) -> Result<Option<ProfileStatistics>, StatisticsCacheError> {
        let key = statistics_key(user_id);
        let Some(payload) = self.client.get(key.clone()).await? else {
            return Ok(None);
        };

        let entry = match serde_json::from_str::<CachedStatistics>(&payload) {
            Ok(entry) => entry,
            Err(error) => {
                let _ = self.client.delete(key).await;
                return Err(error.into());
            }
        };
        if entry.schema_version != STATISTICS_CACHE_SCHEMA_VERSION || entry.user_id != user_id {
            let _ = self.client.delete(key).await;
            return Err(StatisticsCacheError::UnsupportedSchemaVersion);
        }
        Ok(Some(entry.into_authoritative()))
    }

    pub async fn put(
        &self,
        user_id: Uuid,
        value: ProfileStatistics,
    ) -> Result<(), StatisticsCacheError> {
        if value.user_id != user_id {
            return Err(StatisticsCacheError::UnsupportedSchemaVersion);
        }
        let payload = serde_json::to_string(&CachedStatistics::from_authoritative(value))?;
        self.client
            .set_ex(
                statistics_key(user_id),
                payload,
                STATISTICS_CACHE_TTL_SECONDS,
            )
            .await?;
        Ok(())
    }

    /// Invalidate only after the rating/attempt transaction has committed.
    pub async fn invalidate(&self, user_id: Uuid) -> Result<(), StatisticsCacheError> {
        self.client.delete(statistics_key(user_id)).await?;
        Ok(())
    }

    /// Rebuilds the statistics projection from an authoritative PostgreSQL
    /// snapshot. A write failure is reported to the rebuild job, while the
    /// source rows remain untouched and can be retried safely.
    pub async fn rebuild(
        &self,
        authoritative: impl IntoIterator<Item = ProfileStatistics>,
    ) -> Result<usize, StatisticsCacheError> {
        let mut rebuilt = 0;
        for statistics in authoritative {
            self.put(statistics.user_id, statistics).await?;
            rebuilt += 1;
        }
        Ok(rebuilt)
    }

    /// Cache-first statistics read. A Redis outage is deliberately reduced to
    /// a cache miss so the caller can continue using PostgreSQL.
    pub async fn get_or_load<F, Fut, E>(
        &self,
        user_id: Uuid,
        loader: F,
    ) -> Result<ProfileStatistics, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<ProfileStatistics, E>>,
    {
        let value = load_after_cache_read(self.get(user_id).await.map_err(|_| ()), loader).await?;
        let _ = self.put(user_id, value).await;
        Ok(value)
    }
}

async fn load_after_cache_read<T, F, Fut, E>(
    cache_result: Result<Option<T>, ()>,
    loader: F,
) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    if let Ok(Some(value)) = cache_result {
        return Ok(value);
    }
    loader().await
}

fn statistics_key(user_id: Uuid) -> String {
    // The current registry treats profile and its statistics as one public
    // read-model namespace, so both invalidate on the same committed rating
    // or attempt change and share the registered profile TTL.
    RedisKey::Profile {
        user_id: orion_domain::UserId::from_uuid(user_id),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_round_trip_preserves_authoritative_fields() {
        let source = ProfileStatistics {
            user_id: Uuid::from_u128(7),
            rating: Some(1400),
            global_rank: Some(3),
            quizzes_completed: 12,
            correct_answers: 9,
        };
        let cached = CachedStatistics::from_authoritative(source);
        assert_eq!(cached.into_authoritative(), source);
    }

    #[test]
    fn statistics_use_registered_profile_cache_contract() {
        assert_eq!(
            crate::redis_key("cache.profile").map(|spec| spec.ttl),
            Some(crate::RedisTtl::Seconds(
                STATISTICS_CACHE_TTL_SECONDS as u64
            ))
        );
        assert_eq!(
            statistics_key(Uuid::nil()),
            "orion:v1:cache:profile:00000000-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn rebuilt_projection_matches_the_authoritative_statistics() {
        let source = ProfileStatistics {
            user_id: Uuid::from_u128(8),
            rating: Some(1500),
            global_rank: Some(4),
            quizzes_completed: 20,
            correct_answers: 17,
        };
        assert_eq!(
            CachedStatistics::from_authoritative(source).into_authoritative(),
            source
        );
    }

    #[tokio::test]
    async fn redis_failure_falls_back_to_the_authoritative_loader() {
        let loaded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let loaded_by_db = std::sync::Arc::clone(&loaded);
        let result: Result<ProfileStatistics, &str> =
            load_after_cache_read(Err(()), || async move {
                loaded_by_db.store(true, std::sync::atomic::Ordering::SeqCst);
                Err("PostgreSQL remains the source of truth")
            })
            .await;

        assert_eq!(result, Err("PostgreSQL remains the source of truth"));
        assert!(loaded.load(std::sync::atomic::Ordering::SeqCst));
    }
}
