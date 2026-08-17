use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{
    Profile, ProfilePerformanceRow, ProfileStatistics, PublishedProfileResearchRow, RatingEvent,
};

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
          AND qa.status = 'completed'
    ) AS q ON TRUE
    WHERE u.id = $1
      AND u.status = 'active'
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
          AND qa.status = 'completed'
    ) AS q ON TRUE
    WHERE u.username = $1
      AND u.status = 'active'
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
          AND qa.status = 'completed'
    ) AS q ON TRUE
    WHERE u.id = $1
      AND u.status = 'active'
"#;

const CURRENT_ELO_BY_USER_ID: &str = r#"
    SELECT rating
    FROM user_ratings
    WHERE user_id = $1
"#;

const ACTIVE_PROFILE_BY_USER_ID: &str = r#"
    SELECT EXISTS(
        SELECT 1
        FROM users
        WHERE id = $1
          AND status = 'active'
    )
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

const RATING_HISTORY_BY_USER_ID: &str = r#"
    SELECT
        id,
        attempt_id,
        user_id,
        question_id,
        source_type,
        source_id,
        quiz_type,
        outcome,
        correct,
        zone,
        error_pct,
        k,
        sa,
        point_delta,
        user_rating_before,
        user_rating_after,
        player_elo_before,
        player_elo_after,
        question_rating_before,
        question_rating_after,
        question_elo_before,
        question_elo_after,
        rating_delta,
        created_at
    FROM rating_events
    WHERE user_id = $1
    ORDER BY created_at ASC, id ASC
    LIMIT $2
"#;

const PERFORMANCE_HISTORY_BY_USER_ID: &str = r#"
    SELECT
        COALESCE(completed_at, created_at) AS completed_at,
        quiz_type,
        total_questions,
        correct_answers,
        score,
        rating_after
    FROM quiz_attempts
    WHERE user_id = $1
      AND status = 'completed'
    ORDER BY COALESCE(completed_at, created_at) ASC, id ASC
    LIMIT $2
"#;

const PUBLISHED_RESEARCH_BY_USER_ID: &str = r#"
    SELECT
        id,
        title,
        abstract AS abstract_text,
        published_at,
        evaluation_score,
        CASE
            WHEN evaluation_result->>'evaluated_content_version' ~ '^[0-9]+$'
                THEN (evaluation_result->>'evaluated_content_version')::integer
            ELSE NULL
        END AS evaluated_content_version,
        elo_award,
        elo_awarded
    FROM research_papers
    WHERE author_id = $1
      AND status = 'published'
      AND published_at IS NOT NULL
    ORDER BY published_at DESC, id DESC
    LIMIT $2
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

/// Checks the authoritative account lifecycle before serving a cached public
/// projection. Redis must never keep a disabled or deleted user visible.
pub async fn active_user_exists(pool: &PgPool, user_id: Uuid) -> Result<bool> {
    sqlx::query_scalar(ACTIVE_PROFILE_BY_USER_ID)
        .bind(user_id)
        .fetch_one(pool)
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

/// Returns oldest-first immutable rating observations for chart rendering.
pub async fn rating_history_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<RatingEvent>> {
    sqlx::query_as::<_, RatingEvent>(RATING_HISTORY_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Returns oldest-first completed quiz observations. Pending attempts are
/// intentionally excluded so a delayed Advanced settlement cannot appear as
/// completed performance.
pub async fn performance_history_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<ProfilePerformanceRow>> {
    sqlx::query_as::<_, ProfilePerformanceRow>(PERFORMANCE_HISTORY_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Returns only published papers and only the fields safe for a public profile.
pub async fn published_research_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<PublishedProfileResearchRow>> {
    sqlx::query_as::<_, PublishedProfileResearchRow>(PUBLISHED_RESEARCH_BY_USER_ID)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}
