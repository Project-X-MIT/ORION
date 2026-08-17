use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::AdvancedActualRecord;

/// Returns the provider handoff for one question. The table has one current
/// fact per question; PostgreSQL is authoritative and Redis is not involved.
pub async fn by_question_id(
    pool: &PgPool,
    question_id: Uuid,
) -> Result<Option<AdvancedActualRecord>> {
    sqlx::query_as::<_, AdvancedActualRecord>(
        "SELECT question_id, value, observed_at, available_at,
                source_id, source_version, is_final
         FROM advanced_actual_values
         WHERE question_id = $1",
    )
    .bind(question_id)
    .fetch_optional(pool)
    .await
}
