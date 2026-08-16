use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use orion_domain::leaderboard::{
    validate_page_limit, LeaderboardCursor, LeaderboardPageDto, LeaderboardValidationError,
};
use orion_domain::{ContractError, EventEnvelope, RatingUpdatedV1};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::{RedisClient, RedisClientError, RedisKey};

/// Maximum time a cached leaderboard page may be served.
pub const LEADERBOARD_FRESHNESS_BUDGET: Duration = Duration::from_secs(60);

/// Disposable cache for pages produced by the authoritative rank service.
#[derive(Clone)]
pub struct LeaderboardCache {
    client: RedisClient,
    tracked_pages: Arc<Mutex<HashSet<PageCoordinates>>>,
}

impl LeaderboardCache {
    #[must_use]
    pub fn new(client: RedisClient) -> Self {
        Self {
            client,
            tracked_pages: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns a fresh cached page or `None` for a miss, stale value, or corrupt
    /// value. Redis command failures remain distinguishable so API orchestration
    /// can record them and fall back to PostgreSQL.
    pub async fn get(
        &self,
        limit: u32,
        offset: u64,
    ) -> Result<Option<LeaderboardPageDto>, LeaderboardCacheError> {
        let key = validated_key(limit, offset)?;
        let Some(encoded) = self.client.get(key.clone()).await? else {
            return Ok(None);
        };
        let cached: CachedLeaderboardPage = match serde_json::from_str(&encoded) {
            Ok(cached) => cached,
            Err(_) => {
                let _ = self.client.delete(key).await;
                return Ok(None);
            }
        };

        if !cached.is_fresh(unix_timestamp()?) {
            let _ = self.client.delete(key).await;
            return Ok(None);
        }

        self.track(limit, offset).await;
        Ok(Some(cached.page))
    }

    /// Stores an unmodified database-produced page for exactly the registered
    /// freshness budget. The cache does not calculate Elo, rank, movement, or
    /// a replacement `as_of` value.
    pub async fn put(
        &self,
        limit: u32,
        offset: u64,
        page: &LeaderboardPageDto,
    ) -> Result<(), LeaderboardCacheError> {
        let key = validated_key(limit, offset)?;
        let now = unix_timestamp()?;
        let cached = CachedLeaderboardPage {
            cached_at_unix_seconds: now,
            page: page.clone(),
        };
        if !cached.is_fresh(now) {
            return Err(LeaderboardCacheError::StalePage);
        }
        let encoded = serde_json::to_string(&cached)?;
        self.client
            .set_ex(key, encoded, LEADERBOARD_FRESHNESS_BUDGET.as_secs() as i64)
            .await?;
        self.track(limit, offset).await;
        Ok(())
    }

    pub async fn invalidate(&self, limit: u32, offset: u64) -> Result<(), LeaderboardCacheError> {
        self.client.delete(validated_key(limit, offset)?).await?;
        self.tracked_pages
            .lock()
            .await
            .remove(&PageCoordinates { limit, offset });
        Ok(())
    }

    /// Invalidates every page observed by this process after a compatible,
    /// committed rating event, then stores pages refreshed from PostgreSQL.
    pub async fn on_rating_updated(
        &self,
        event: &EventEnvelope<RatingUpdatedV1>,
        refreshed_pages: &[RefreshedLeaderboardPage],
    ) -> Result<(), LeaderboardCacheError> {
        event.validate_contract()?;
        self.invalidate_tracked().await?;
        self.store_refreshed(refreshed_pages).await
    }

    /// Invalidates only after a snapshot transaction inserted rows. A
    /// duplicate or backdated no-op snapshot leaves the cache untouched.
    pub async fn after_snapshot_commit(
        &self,
        rows_affected: u64,
        refreshed_pages: &[RefreshedLeaderboardPage],
    ) -> Result<(), LeaderboardCacheError> {
        if rows_affected == 0 {
            return Ok(());
        }
        self.invalidate_tracked().await?;
        self.store_refreshed(refreshed_pages).await
    }

    async fn store_refreshed(
        &self,
        pages: &[RefreshedLeaderboardPage],
    ) -> Result<(), LeaderboardCacheError> {
        for page in pages {
            self.put(page.limit, page.offset, &page.page).await?;
        }
        Ok(())
    }

    async fn invalidate_tracked(&self) -> Result<(), LeaderboardCacheError> {
        let pages: Vec<_> = self.tracked_pages.lock().await.drain().collect();
        let mut first_error = None;
        for page in pages {
            if let Err(error) = self
                .client
                .delete(validated_key(page.limit, page.offset)?)
                .await
            {
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }

    async fn track(&self, limit: u32, offset: u64) {
        self.tracked_pages
            .lock()
            .await
            .insert(PageCoordinates { limit, offset });
    }
}

/// A page re-queried from PostgreSQL after an invalidating event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshedLeaderboardPage {
    pub limit: u32,
    pub offset: u64,
    pub page: LeaderboardPageDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PageCoordinates {
    limit: u32,
    offset: u64,
}

#[derive(Debug, Error)]
pub enum LeaderboardCacheError {
    #[error(transparent)]
    Validation(#[from] LeaderboardValidationError),
    #[error("leaderboard event contract is invalid")]
    EventContract(#[from] ContractError),
    #[error("leaderboard cache command failed")]
    Redis(#[from] RedisClientError),
    #[error("leaderboard cache serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error("system clock is before the Unix epoch")]
    InvalidSystemClock,
    #[error("leaderboard page is already outside the freshness budget")]
    StalePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedLeaderboardPage {
    cached_at_unix_seconds: u64,
    page: LeaderboardPageDto,
}

impl CachedLeaderboardPage {
    fn is_fresh(&self, now_unix_seconds: u64) -> bool {
        is_within_budget(now_unix_seconds, self.cached_at_unix_seconds)
            && u64::try_from(self.page.as_of.timestamp())
                .ok()
                .is_some_and(|as_of| is_within_budget(now_unix_seconds, as_of))
    }
}

fn is_within_budget(now_unix_seconds: u64, timestamp: u64) -> bool {
    now_unix_seconds
        .checked_sub(timestamp)
        .is_some_and(|age| age < LEADERBOARD_FRESHNESS_BUDGET.as_secs())
}

fn validated_key(limit: u32, offset: u64) -> Result<String, LeaderboardValidationError> {
    validate_page_limit(limit)?;
    LeaderboardCursor::new(offset)?;
    Ok(RedisKey::Leaderboard { limit, offset }.to_string())
}

fn unix_timestamp() -> Result<u64, LeaderboardCacheError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| LeaderboardCacheError::InvalidSystemClock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_at(timestamp: u64) -> CachedLeaderboardPage {
        CachedLeaderboardPage {
            cached_at_unix_seconds: timestamp,
            page: serde_json::from_value(serde_json::json!({
                "entries": [],
                "next_cursor": null,
                "as_of": "1970-01-01T00:16:40Z"
            }))
            .unwrap(),
        }
    }

    #[test]
    fn freshness_is_strictly_bounded_to_registered_ttl() {
        let cached = cached_at(1_000);

        assert!(cached.is_fresh(1_059));
        assert!(!cached.is_fresh(1_060));
        assert!(!cached.is_fresh(999));
    }

    #[test]
    fn reinsertion_cannot_make_an_old_authoritative_page_fresh() {
        let mut cached = cached_at(1_000);
        cached.cached_at_unix_seconds = 1_059;

        assert!(!cached.is_fresh(1_060));
    }

    #[test]
    fn registered_key_is_used_after_shared_validation() {
        assert_eq!(
            crate::redis_key("cache.leaderboard").map(|spec| spec.ttl),
            Some(crate::RedisTtl::Seconds(
                LEADERBOARD_FRESHNESS_BUDGET.as_secs()
            ))
        );
        assert_eq!(
            validated_key(20, 40).unwrap(),
            "orion:v1:cache:leaderboard:20:40"
        );
        assert!(matches!(
            validated_key(0, 0),
            Err(LeaderboardValidationError::InvalidLimit(0))
        ));
    }
}
