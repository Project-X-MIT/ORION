use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{LeaderboardEntry, LeaderboardRankHistory};

const GLOBAL_LEADERBOARD: &str = r#"
    WITH ranked_users AS (
        SELECT
            user_id,
            rating,
            ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
        FROM user_ratings
    )
    SELECT
        ranked.rank,
        u.id AS user_id,
        u.username,
        u.display_name,
        u.avatar_url,
        ranked.rating
    FROM ranked_users AS ranked
    INNER JOIN users AS u ON u.id = ranked.user_id
    ORDER BY ranked.rating DESC, ranked.user_id ASC
    LIMIT $1
    OFFSET $2
"#;

const LATEST_RANK_MOVEMENT_BY_USER_ID: &str = r#"
    SELECT
        snapshot_at,
        user_id,
        previous_rank,
        current_rank,
        rank_movement
    FROM leaderboard_rank_history
    WHERE user_id = $1
    ORDER BY snapshot_at DESC
    LIMIT 1
"#;

const RANK_HISTORY_BY_USER_ID: &str = r#"
    SELECT
        snapshot_at,
        user_id,
        previous_rank,
        current_rank,
        rank_movement
    FROM leaderboard_rank_history
    WHERE user_id = $1
    ORDER BY snapshot_at DESC
    LIMIT $2
    OFFSET $3
"#;

/// Returns one page of the global leaderboard.
///
/// Users without a `user_ratings` row are not ranked. Results are sorted by
/// current Elo descending; equal ratings are ordered by immutable user ID
/// ascending so every user has a stable, unique position.
pub async fn global_leaderboard(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<LeaderboardEntry>> {
    sqlx::query_as::<_, LeaderboardEntry>(GLOBAL_LEADERBOARD)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Returns a user's most recent persisted rank and calculated movement.
pub async fn latest_rank_movement_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<LeaderboardRankHistory>> {
    sqlx::query_as::<_, LeaderboardRankHistory>(LATEST_RANK_MOVEMENT_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// Returns a newest-first page of a user's historical leaderboard positions.
pub async fn rank_history_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<LeaderboardRankHistory>> {
    sqlx::query_as::<_, LeaderboardRankHistory>(RANK_HISTORY_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}
