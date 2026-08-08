use sqlx::{Postgres, Result, Transaction};
use uuid::Uuid;

/// Applies one Elo delta using the shared rating transaction path.
///
/// The current rating row is updated atomically.  If the author has no row
/// yet, the standard 1000 baseline is created before applying the award.
pub async fn apply_elo_delta(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    delta: i32,
) -> Result<i32> {
    sqlx::query_scalar::<_, i32>(
        "INSERT INTO user_ratings (user_id, rating)\n         VALUES ($1, 1000 + $2)\n         ON CONFLICT (user_id) DO UPDATE\n         SET rating = user_ratings.rating + EXCLUDED.rating - 1000\n         RETURNING rating",
    )
    .bind(user_id)
    .bind(delta)
    .fetch_one(&mut **transaction)
    .await
}

pub async fn award_elo(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    award: i32,
) -> Result<i32> {
    apply_elo_delta(transaction, user_id, award).await
}
