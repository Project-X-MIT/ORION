use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use orion_db::{
    models::{LeaderboardEntry, LeaderboardRankHistory},
    queries::leaderboard::{
        global_leaderboard, latest_rank_movement_by_user_id, rank_history_by_user_id,
    },
};
use orion_domain::{
    leaderboard::{
        validate_database_rank, validate_database_rating, validate_page_limit, LeaderboardCursor,
        LeaderboardEntryDto, LeaderboardPageDto, LeaderboardValidationError, RankHistoryEntryDto,
        RankHistoryPageDto,
    },
    UserId,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;

use orion_redis::{cache::leaderboard::LeaderboardCache, RedisClient};

const CURSOR_VERSION: u8 = 1;

/// Reads authoritative ranks through the completed DB-03 query surface.
///
/// This service deliberately has no Redis dependency. Cache lookup and refresh
/// belong to the HTTP/cache adapter; a cache miss delegates here to PostgreSQL.
#[derive(Clone)]
pub struct RankService {
    pool: PgPool,
}

/// Cache-first leaderboard reader with PostgreSQL fallback.
#[derive(Clone)]
pub struct LeaderboardService {
    ranks: RankService,
    cache: Option<LeaderboardCache>,
}

impl LeaderboardService {
    #[must_use]
    pub fn with_cache(pool: PgPool, redis: RedisClient) -> Self {
        Self {
            ranks: RankService::new(pool),
            cache: Some(LeaderboardCache::new(redis)),
        }
    }

    /// Constructs the normal degraded mode used while Redis is unavailable.
    #[must_use]
    pub const fn without_cache(pool: PgPool) -> Self {
        Self {
            ranks: RankService::new(pool),
            cache: None,
        }
    }

    pub async fn global_page(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<LeaderboardPageDto, RankServiceError> {
        validate_page_limit(limit)?;
        let offset = cursor
            .map(decode_cursor)
            .transpose()?
            .map_or(0, LeaderboardCursor::next_offset);

        if let Some(cache) = &self.cache {
            if let Ok(Some(page)) = cache.get(limit, offset).await {
                return Ok(page);
            }
        }

        let page = self.ranks.global_page(limit, cursor).await?;
        if let Some(cache) = &self.cache {
            // Redis is disposable. A failed refresh must not hide a successful
            // authoritative read.
            let _ = cache.put(limit, offset, &page).await;
        }
        Ok(page)
    }
}

impl RankService {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns one ordered page and an opaque cursor for the following page.
    pub async fn global_page(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<LeaderboardPageDto, RankServiceError> {
        validate_page_limit(limit)?;

        let page_cursor = cursor.map(decode_cursor).transpose()?;
        let offset = page_cursor.map_or(0, LeaderboardCursor::next_offset);
        let query_limit = i64::from(limit) + 1;
        let query_offset =
            i64::try_from(offset).map_err(|_| LeaderboardValidationError::InvalidCursor)?;
        let mut rows = global_leaderboard(&self.pool, query_limit, query_offset).await?;
        let has_next_page = rows.len() > limit as usize;
        rows.truncate(limit as usize);

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.to_dto(row).await?);
        }

        let next_cursor = if has_next_page {
            let cursor = LeaderboardCursor::new(offset)?.advance(limit)?;
            Some(encode_cursor(cursor)?)
        } else {
            None
        };

        Ok(LeaderboardPageDto {
            entries,
            next_cursor,
            as_of: Utc::now(),
        })
    }

    /// Returns the latest completed historical rank snapshot for a user.
    pub async fn latest_movement(
        &self,
        user_id: UserId,
    ) -> Result<Option<RankHistoryEntryDto>, RankServiceError> {
        latest_rank_movement_by_user_id(&self.pool, user_id.into_uuid())
            .await?
            .map(history_to_dto)
            .transpose()
    }

    /// Returns a newest-first page of completed historical rank snapshots.
    pub async fn rank_history(
        &self,
        user_id: UserId,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<RankHistoryPageDto, RankServiceError> {
        validate_page_limit(limit)?;

        let page_cursor = cursor.map(decode_cursor).transpose()?;
        let offset = page_cursor.map_or(0, LeaderboardCursor::next_offset);
        let query_offset =
            i64::try_from(offset).map_err(|_| LeaderboardValidationError::InvalidCursor)?;
        let mut rows = rank_history_by_user_id(
            &self.pool,
            user_id.into_uuid(),
            i64::from(limit) + 1,
            query_offset,
        )
        .await?;
        let has_next_page = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let entries = rows
            .into_iter()
            .map(history_to_dto)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if has_next_page {
            let cursor = LeaderboardCursor::new(offset)?.advance(limit)?;
            Some(encode_cursor(cursor)?)
        } else {
            None
        };

        Ok(RankHistoryPageDto {
            entries,
            next_cursor,
        })
    }

    async fn to_dto(&self, row: LeaderboardEntry) -> Result<LeaderboardEntryDto, RankServiceError> {
        let rank = validate_database_rank(row.rank)?;
        let rating = validate_database_rating(row.rating)?;
        let movement = self.latest_movement(UserId::from_uuid(row.user_id)).await?;

        Ok(LeaderboardEntryDto {
            rank,
            user_id: UserId::from_uuid(row.user_id),
            username: row.username,
            display_name: row.display_name,
            avatar_url: row.avatar_url,
            rating,
            rank_movement: movement.and_then(|snapshot| snapshot.rank_movement),
        })
    }
}

fn history_to_dto(row: LeaderboardRankHistory) -> Result<RankHistoryEntryDto, RankServiceError> {
    let current_rank = validate_database_rank(row.current_rank)?;
    let previous_rank = row.previous_rank.map(validate_database_rank).transpose()?;
    Ok(RankHistoryEntryDto {
        snapshot_at: row.snapshot_at,
        user_id: UserId::from_uuid(row.user_id),
        previous_rank,
        current_rank,
        rank_movement: row.rank_movement,
    })
}

#[derive(Debug, Error)]
pub enum RankServiceError {
    #[error(transparent)]
    Validation(#[from] LeaderboardValidationError),
    #[error("leaderboard database query failed")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorEnvelope {
    version: u8,
    next_offset: u64,
}

fn encode_cursor(cursor: LeaderboardCursor) -> Result<String, RankServiceError> {
    let payload = serde_json::to_vec(&CursorEnvelope {
        version: CURSOR_VERSION,
        next_offset: cursor.next_offset(),
    })
    .map_err(|_| LeaderboardValidationError::InvalidCursor)?;
    Ok(URL_SAFE_NO_PAD.encode(payload))
}

fn decode_cursor(encoded: &str) -> Result<LeaderboardCursor, RankServiceError> {
    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| LeaderboardValidationError::InvalidCursor)?;
    let envelope: CursorEnvelope =
        serde_json::from_slice(&payload).map_err(|_| LeaderboardValidationError::InvalidCursor)?;
    if envelope.version != CURSOR_VERSION {
        return Err(LeaderboardValidationError::InvalidCursor.into());
    }
    Ok(LeaderboardCursor::new(envelope.next_offset)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip_preserves_next_offset() {
        let encoded = encode_cursor(LeaderboardCursor::new(120).unwrap()).unwrap();
        assert_eq!(decode_cursor(&encoded).unwrap().next_offset(), 120);
    }

    #[test]
    fn cursor_rejects_malformed_and_unknown_versions() {
        assert!(matches!(
            decode_cursor("not-base64!"),
            Err(RankServiceError::Validation(
                LeaderboardValidationError::InvalidCursor
            ))
        ));

        let unknown = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorEnvelope {
                version: CURSOR_VERSION + 1,
                next_offset: 20,
            })
            .unwrap(),
        );
        assert!(matches!(
            decode_cursor(&unknown),
            Err(RankServiceError::Validation(
                LeaderboardValidationError::InvalidCursor
            ))
        ));
    }
}
