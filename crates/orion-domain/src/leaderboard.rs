//! Transport-neutral contracts for the global Elo leaderboard.
//!
//! PostgreSQL remains authoritative for both Elo and rank. These types describe
//! a deterministic view of that data; they do not calculate or persist either.

use chrono::{DateTime, Utc};
use orion_common::MAX_PAGE_SIZE;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Rating, UserId};

/// One user in the global leaderboard response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntryDto {
    /// Unique, contiguous, one-based position calculated by PostgreSQL.
    pub rank: u64,
    pub user_id: UserId,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    /// Current authoritative Elo read from `user_ratings`.
    pub rating: Rating,
    /// Change from the latest completed rank snapshot. Positive means up.
    pub rank_movement: Option<i64>,
}

/// A page of the deterministic global leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardPageDto {
    pub entries: Vec<LeaderboardEntryDto>,
    /// Opaque, versioned cursor for the next page; absent on the final page.
    pub next_cursor: Option<String>,
    /// Time at which PostgreSQL produced this leaderboard view.
    ///
    /// A cache hit must preserve this value so callers can observe bounded
    /// staleness; refreshing a cache entry must not fabricate a newer value.
    pub as_of: DateTime<Utc>,
}

/// One completed, immutable rank snapshot for a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankHistoryEntryDto {
    pub snapshot_at: DateTime<Utc>,
    pub user_id: UserId,
    pub previous_rank: Option<u64>,
    pub current_rank: u64,
    /// Positive means up, negative means down, and zero means unchanged.
    pub rank_movement: Option<i64>,
}

/// A newest-first page of completed rank snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankHistoryPageDto {
    pub entries: Vec<RankHistoryEntryDto>,
    pub next_cursor: Option<String>,
}

/// The next offset in the authoritative ordered result.
///
/// API adapters encode this value as an opaque, versioned string. It identifies
/// a read boundary only and never represents an authoritative rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardCursor {
    next_offset: u64,
}

impl LeaderboardCursor {
    pub const fn new(next_offset: u64) -> Result<Self, LeaderboardValidationError> {
        if next_offset > i64::MAX as u64 {
            return Err(LeaderboardValidationError::InvalidCursor);
        }
        Ok(Self { next_offset })
    }

    #[must_use]
    pub const fn next_offset(self) -> u64 {
        self.next_offset
    }

    pub const fn advance(self, count: u32) -> Result<Self, LeaderboardValidationError> {
        match self.next_offset.checked_add(count as u64) {
            Some(next_offset) => Self::new(next_offset),
            None => Err(LeaderboardValidationError::InvalidCursor),
        }
    }
}

/// Validation failures shared by leaderboard transports and adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LeaderboardValidationError {
    #[error("leaderboard limit must be between 1 and {MAX_PAGE_SIZE}, got {0}")]
    InvalidLimit(u32),
    #[error("leaderboard cursor is malformed, overflowing, or unsupported")]
    InvalidCursor,
    #[error("leaderboard rank must be positive, got {0}")]
    InvalidRank(i64),
    #[error("authoritative Elo must be non-negative, got {0}")]
    InvalidRating(i32),
}

pub const fn validate_page_limit(limit: u32) -> Result<(), LeaderboardValidationError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(LeaderboardValidationError::InvalidLimit(limit));
    }
    Ok(())
}

pub fn validate_database_rank(rank: i64) -> Result<u64, LeaderboardValidationError> {
    u64::try_from(rank)
        .ok()
        .filter(|rank| *rank > 0)
        .ok_or(LeaderboardValidationError::InvalidRank(rank))
}

pub fn validate_database_rating(rating: i32) -> Result<Rating, LeaderboardValidationError> {
    match Rating::new(rating) {
        Ok(rating) => Ok(rating),
        Err(_) => Err(LeaderboardValidationError::InvalidRating(rating)),
    }
}

/// Stable ordering contract shared by the query, cursor, cache, and response.
///
/// 1. Higher authoritative `user_ratings.rating` sorts first.
/// 2. Equal ratings sort by immutable `users.id` ascending.
/// 3. Rank is `ROW_NUMBER()` over that total order, so it is unique and
///    one-based. Username, profile fields, rank history, and cache state never
///    participate in ordering.
pub const LEADERBOARD_ORDER_SQL: &str = "rating DESC, user_id ASC";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_only_a_read_offset() {
        assert_eq!(LeaderboardCursor::new(40).unwrap().next_offset(), 40);
    }

    #[test]
    fn validation_rejects_invalid_limits_ranks_ratings_and_offsets() {
        assert_eq!(
            validate_page_limit(0),
            Err(LeaderboardValidationError::InvalidLimit(0))
        );
        assert_eq!(
            validate_database_rank(0),
            Err(LeaderboardValidationError::InvalidRank(0))
        );
        assert_eq!(
            validate_database_rating(-1),
            Err(LeaderboardValidationError::InvalidRating(-1))
        );
        assert_eq!(
            LeaderboardCursor::new(i64::MAX as u64 + 1),
            Err(LeaderboardValidationError::InvalidCursor)
        );
    }
}
