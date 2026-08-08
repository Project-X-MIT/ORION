use chrono::{DateTime, Utc};
use sqlx::{PgPool, Result};

const INSERT_LEADERBOARD_SNAPSHOT: &str = r#"
    WITH snapshot_guard AS (
        SELECT 1
        WHERE NOT EXISTS (
            SELECT 1
            FROM leaderboard_rank_history
            WHERE snapshot_at > $1
        )
    ),
    ranked_users AS (
        SELECT
            user_id,
            rating,
            ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
        FROM user_ratings
    ),
    previous_snapshot AS (
        SELECT MAX(snapshot_at) AS snapshot_at
        FROM leaderboard_rank_history
        WHERE snapshot_at < $1
    ),
    previous_ranks AS (
        SELECT
            history.user_id,
            history.current_rank
        FROM leaderboard_rank_history AS history
        INNER JOIN previous_snapshot
            ON previous_snapshot.snapshot_at = history.snapshot_at
    )
    INSERT INTO leaderboard_rank_history (
        snapshot_at,
        user_id,
        previous_rank,
        current_rank
    )
    SELECT
        $1,
        ranked.user_id,
        previous.current_rank,
        ranked.rank
    FROM ranked_users AS ranked
    CROSS JOIN snapshot_guard
    LEFT JOIN previous_ranks AS previous ON previous.user_id = ranked.user_id
    ON CONFLICT (snapshot_at, user_id) DO NOTHING
"#;

/// Persists an immutable global-leaderboard snapshot atomically.
///
/// `user_ratings` is read directly as the authoritative current-Elo source.
/// Repeating the transaction for the same timestamp is safe because existing
/// snapshot rows are preserved. A backdated snapshot is ignored when newer
/// history already exists. The returned value is the number of rows inserted.
pub async fn snapshot_leaderboard(pool: &PgPool, snapshot_at: DateTime<Utc>) -> Result<u64> {
    let mut transaction = pool.begin().await?;

    // Serialize snapshot writers while still allowing ordinary history reads.
    // This guarantees that `previous_rank` is derived from the snapshot that
    // immediately precedes this one.
    sqlx::query("LOCK TABLE leaderboard_rank_history IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *transaction)
        .await?;

    let result = sqlx::query(INSERT_LEADERBOARD_SNAPSHOT)
        .bind(snapshot_at)
        .execute(&mut *transaction)
        .await?;

    transaction.commit().await?;

    Ok(result.rows_affected())
}
