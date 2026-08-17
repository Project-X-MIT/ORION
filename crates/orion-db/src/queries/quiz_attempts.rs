use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{AdvancedPredictionRecord, NewQuizAttempt, QuizAttempt, ATTEMPT_PENDING};

const ATTEMPT_BY_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE id = $1
"#;

const ATTEMPTS_BY_USER_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE user_id = $1
    ORDER BY created_at DESC, id DESC
    LIMIT $2
    OFFSET $3
"#;

const LATEST_ATTEMPT_BY_USER_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE user_id = $1
    ORDER BY created_at DESC, id DESC
    LIMIT 1
"#;

const PENDING_ADVANCED_BY_USER_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE user_id = $1
      AND quiz_type = 'advanced'
      AND status = 'pending'
    ORDER BY created_at DESC, id DESC
    LIMIT $2
    OFFSET $3
"#;

const PENDING_ADVANCED_BY_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE id = $1
      AND user_id = $2
      AND quiz_type = 'advanced'
      AND status = 'pending'
"#;

const COMPLETED_ATTEMPTS_BY_USER_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE user_id = $1
      AND status = 'completed'
    ORDER BY completed_at DESC, id DESC
    LIMIT $2
    OFFSET $3
"#;

const COMPLETED_ATTEMPT_BY_ID: &str = r#"
    SELECT
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
    FROM quiz_attempts
    WHERE id = $1
      AND user_id = $2
      AND status = 'completed'
"#;

const INSERT_PENDING_ATTEMPT: &str = r#"
    INSERT INTO quiz_attempts (
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        created_at,
        updated_at
    )
    VALUES ($1, $2, $3, $4, $5, 0, 0, $6, $6, $7, $7, $7)
    RETURNING
        id,
        user_id,
        quiz_type,
        status,
        total_questions,
        correct_answers,
        score,
        rating_before,
        rating_after,
        started_at,
        completed_at,
        created_at,
        updated_at
"#;

/// Creates a pending attempt with its initial user-rating snapshot.
pub async fn create(pool: &PgPool, input: &NewQuizAttempt) -> Result<QuizAttempt> {
    sqlx::query_as::<_, QuizAttempt>(INSERT_PENDING_ATTEMPT)
        .bind(input.id)
        .bind(input.user_id)
        .bind(input.quiz_type.as_str())
        .bind(ATTEMPT_PENDING)
        .bind(input.total_questions)
        .bind(input.rating_before)
        .bind(input.started_at)
        .fetch_one(pool)
        .await
}

/// Explicit alias for callers that want to emphasize the lifecycle state.
pub async fn create_pending(pool: &PgPool, input: &NewQuizAttempt) -> Result<QuizAttempt> {
    create(pool, input).await
}

pub async fn find_by_id(pool: &PgPool, attempt_id: Uuid) -> Result<Option<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(ATTEMPT_BY_ID)
        .bind(attempt_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(ATTEMPTS_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn latest_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Option<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(LATEST_ATTEMPT_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// Returns a user's pending Advanced Quiz attempts, newest first.
pub async fn pending_advanced_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(PENDING_ADVANCED_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Returns the newest pending Advanced Quiz attempt for a user, if any.
pub async fn find_pending_advanced_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<QuizAttempt>> {
    Ok(pending_advanced_by_user_id(pool, user_id, 1, 0)
        .await?
        .into_iter()
        .next())
}

/// Returns one pending Advanced Quiz attempt owned by the user.
pub async fn find_pending_advanced_by_id(
    pool: &PgPool,
    attempt_id: Uuid,
    user_id: Uuid,
) -> Result<Option<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(PENDING_ADVANCED_BY_ID)
        .bind(attempt_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// Loads the exact numeric predictions associated with one pending attempt.
/// PostgreSQL is authoritative; this is the worker's DB-02 handoff surface.
pub async fn advanced_predictions_by_attempt_id(
    pool: &PgPool,
    attempt_id: Uuid,
) -> Result<Vec<AdvancedPredictionRecord>> {
    sqlx::query_as::<_, AdvancedPredictionRecord>(
        "SELECT attempt_id, question_id, value, submitted_at
         FROM advanced_predictions
         WHERE attempt_id = $1
         ORDER BY question_id ASC",
    )
    .bind(attempt_id)
    .fetch_all(pool)
    .await
}

/// Returns a user's completed attempts, newest completion first.
pub async fn completed_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(COMPLETED_ATTEMPTS_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Alias using the plural resource name used by history endpoints.
pub async fn completed_attempts_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizAttempt>> {
    completed_by_user_id(pool, user_id, limit, offset).await
}

/// Returns one completed attempt owned by the user.
pub async fn find_completed_by_id(
    pool: &PgPool,
    attempt_id: Uuid,
    user_id: Uuid,
) -> Result<Option<QuizAttempt>> {
    sqlx::query_as::<_, QuizAttempt>(COMPLETED_ATTEMPT_BY_ID)
        .bind(attempt_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn completed_count_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM quiz_attempts
        WHERE user_id = $1
          AND status = 'completed'
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}
