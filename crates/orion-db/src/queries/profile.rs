use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{Profile, ProfileStatistics};

const PROFILE_BY_USER_ID: &str = r#"
    WITH current_ratings AS (
        SELECT user_id, rating
        FROM user_ratings
    ),
    ranked_users AS (
        SELECT
            user_id,
            rating,
            ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS global_rank
        FROM current_ratings
    )
    SELECT
        u.id AS user_id,
        u.username,
        u.display_name,
        u.bio,
        u.avatar_url,
        r.rating,
        r.global_rank,
        COALESCE(q.quizzes_completed, 0)::bigint AS quizzes_completed,
        COALESCE(q.correct_answers, 0)::bigint AS correct_answers,
        u.created_at,
        u.updated_at
    FROM users AS u
    LEFT JOIN ranked_users AS r ON r.user_id = u.id
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*) AS quizzes_completed,
            COALESCE(SUM(qa.correct_answers), 0)::bigint AS correct_answers
        FROM quiz_attempts AS qa
        WHERE qa.user_id = u.id
    ) AS q ON TRUE
    WHERE u.id = $1
"#;

const PROFILE_BY_USERNAME: &str = r#"
    WITH current_ratings AS (
        SELECT user_id, rating
        FROM user_ratings
    ),
    ranked_users AS (
        SELECT
            user_id,
            rating,
            ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS global_rank
        FROM current_ratings
    )
    SELECT
        u.id AS user_id,
        u.username,
        u.display_name,
        u.bio,
        u.avatar_url,
        r.rating,
        r.global_rank,
        COALESCE(q.quizzes_completed, 0)::bigint AS quizzes_completed,
        COALESCE(q.correct_answers, 0)::bigint AS correct_answers,
        u.created_at,
        u.updated_at
    FROM users AS u
    LEFT JOIN ranked_users AS r ON r.user_id = u.id
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*) AS quizzes_completed,
            COALESCE(SUM(qa.correct_answers), 0)::bigint AS correct_answers
        FROM quiz_attempts AS qa
        WHERE qa.user_id = u.id
    ) AS q ON TRUE
    WHERE u.username = $1
"#;

const PROFILE_STATISTICS_BY_USER_ID: &str = r#"
    WITH current_ratings AS (
        SELECT user_id, rating
        FROM user_ratings
    ),
    ranked_users AS (
        SELECT
            user_id,
            rating,
            ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS global_rank
        FROM current_ratings
    )
    SELECT
        u.id AS user_id,
        r.rating,
        r.global_rank,
        COALESCE(q.quizzes_completed, 0)::bigint AS quizzes_completed,
        COALESCE(q.correct_answers, 0)::bigint AS correct_answers
    FROM users AS u
    LEFT JOIN ranked_users AS r ON r.user_id = u.id
    LEFT JOIN LATERAL (
        SELECT
            COUNT(*) AS quizzes_completed,
            COALESCE(SUM(qa.correct_answers), 0)::bigint AS correct_answers
        FROM quiz_attempts AS qa
        WHERE qa.user_id = u.id
    ) AS q ON TRUE
    WHERE u.id = $1
"#;

const CURRENT_ELO_BY_USER_ID: &str = r#"
    SELECT rating
    FROM user_ratings
    WHERE user_id = $1
"#;

const CURRENT_RANK_BY_USER_ID: &str = r#"
    WITH current_ratings AS (
        SELECT user_id, rating
        FROM user_ratings
    ),
    target_user AS (
        SELECT user_id, rating
        FROM current_ratings
        WHERE user_id = $1
    )
    SELECT (
        SELECT COUNT(*) + 1
        FROM current_ratings AS other_user
        WHERE other_user.rating > target_user.rating
           OR (
               other_user.rating = target_user.rating
               AND other_user.user_id < target_user.user_id
           )
    )::bigint
    FROM target_user
"#;

pub async fn find_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Option<Profile>> {
    sqlx::query_as::<_, Profile>(PROFILE_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<Profile>> {
    sqlx::query_as::<_, Profile>(PROFILE_BY_USERNAME)
        .bind(username)
        .fetch_optional(pool)
        .await
}

pub async fn statistics_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<ProfileStatistics>> {
    sqlx::query_as::<_, ProfileStatistics>(PROFILE_STATISTICS_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// Returns the user's most recently persisted Elo rating.
pub async fn current_elo_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Option<i32>> {
    sqlx::query_scalar(CURRENT_ELO_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// Returns the user's current global rank based on the latest Elo per user.
/// Equal ratings are resolved by user ID ascending, matching the leaderboard.
pub async fn current_rank_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Option<i64>> {
    sqlx::query_scalar(CURRENT_RANK_BY_USER_ID)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}
