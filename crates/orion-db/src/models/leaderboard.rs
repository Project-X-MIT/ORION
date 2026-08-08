use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A user positioned on the global Elo leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub user_id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub rating: i32,
}

/// A user's position and movement captured in one leaderboard snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct LeaderboardRankHistory {
    pub snapshot_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub previous_rank: Option<i64>,
    pub current_rank: i64,
    /// Positive means up, negative means down, and zero means unchanged.
    pub rank_movement: Option<i64>,
}
