use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{
    CourseCompletion, CourseLesson, CourseModule, CourseProgress, ModuleCompletion,
};

const PUBLISHED_MODULES: &str = r#"
    SELECT id, slug, title, description, display_order, is_published,
           created_at, updated_at
    FROM course_modules
    WHERE is_published = TRUE
    ORDER BY display_order ASC, id ASC
"#;

const MODULE_BY_ID: &str = r#"
    SELECT id, slug, title, description, display_order, is_published,
           created_at, updated_at
    FROM course_modules
    WHERE id = $1 AND is_published = TRUE
"#;

const MODULE_BY_SLUG: &str = r#"
    SELECT id, slug, title, description, display_order, is_published,
           created_at, updated_at
    FROM course_modules
    WHERE slug = $1 AND is_published = TRUE
"#;

const PUBLISHED_LESSONS_BY_MODULE_ID: &str = r#"
    SELECT l.id, l.module_id, l.slug, l.title, l.summary, l.content,
           l.lesson_order, l.estimated_minutes, l.is_published,
           l.created_at, l.updated_at
    FROM course_lessons AS l
    INNER JOIN course_modules AS m ON m.id = l.module_id
    WHERE l.module_id = $1
      AND l.is_published = TRUE
      AND m.is_published = TRUE
    ORDER BY l.lesson_order ASC, l.id ASC
"#;

const LESSON_BY_ID: &str = r#"
    SELECT l.id, l.module_id, l.slug, l.title, l.summary, l.content,
           l.lesson_order, l.estimated_minutes, l.is_published,
           l.created_at, l.updated_at
    FROM course_lessons AS l
    INNER JOIN course_modules AS m ON m.id = l.module_id
    WHERE l.id = $1
      AND l.is_published = TRUE
      AND m.is_published = TRUE
"#;

const LESSON_BY_MODULE_AND_SLUG: &str = r#"
    SELECT l.id, l.module_id, l.slug, l.title, l.summary, l.content,
           l.lesson_order, l.estimated_minutes, l.is_published,
           l.created_at, l.updated_at
    FROM course_lessons AS l
    INNER JOIN course_modules AS m ON m.id = l.module_id
    WHERE l.module_id = $1
      AND l.slug = $2
      AND l.is_published = TRUE
      AND m.is_published = TRUE
"#;

const PROGRESS_BY_USER_ID: &str = r#"
    SELECT user_id, lesson_id, completed, started_at, completed_at,
           last_accessed_at, updated_at
    FROM course_progress
    WHERE user_id = $1
    ORDER BY last_accessed_at DESC NULLS LAST, lesson_id ASC
"#;

const PROGRESS_BY_USER_AND_LESSON: &str = r#"
    SELECT user_id, lesson_id, completed, started_at, completed_at,
           last_accessed_at, updated_at
    FROM course_progress
    WHERE user_id = $1 AND lesson_id = $2
"#;

const MODULE_COMPLETION_BY_USER: &str = r#"
    SELECT
        $1::uuid AS user_id,
        m.id AS module_id,
        COUNT(l.id)::bigint AS total_lessons,
        COUNT(l.id) FILTER (WHERE p.completed = TRUE)::bigint AS completed_lessons,
        (
            COUNT(l.id) > 0
            AND COUNT(l.id) FILTER (WHERE p.completed = TRUE) = COUNT(l.id)
        ) AS completed
    FROM course_modules AS m
    LEFT JOIN course_lessons AS l
        ON l.module_id = m.id
       AND l.is_published = TRUE
    LEFT JOIN course_progress AS p
        ON p.lesson_id = l.id
       AND p.user_id = $1
    WHERE m.id = $2
      AND m.is_published = TRUE
    GROUP BY m.id
"#;

const COURSE_COMPLETION_BY_USER: &str = r#"
    WITH module_stats AS (
        SELECT
            m.id AS module_id,
            COUNT(l.id)::bigint AS total_lessons,
            COUNT(l.id) FILTER (WHERE p.completed = TRUE)::bigint
                AS completed_lessons
        FROM course_modules AS m
        LEFT JOIN course_lessons AS l
            ON l.module_id = m.id
           AND l.is_published = TRUE
        LEFT JOIN course_progress AS p
            ON p.lesson_id = l.id
           AND p.user_id = $1
        WHERE m.is_published = TRUE
        GROUP BY m.id
    )
    SELECT
        $1::uuid AS user_id,
        COUNT(*)::bigint AS total_modules,
        COUNT(*) FILTER (
            WHERE total_lessons > 0
              AND completed_lessons = total_lessons
        )::bigint AS completed_modules,
        COALESCE(SUM(total_lessons), 0)::bigint AS total_lessons,
        COALESCE(SUM(completed_lessons), 0)::bigint AS completed_lessons,
        (
            COUNT(*) > 0
            AND COUNT(*) FILTER (
                WHERE total_lessons > 0
                  AND completed_lessons = total_lessons
            ) = COUNT(*)
        ) AS completed
    FROM module_stats
"#;

const UPSERT_PROGRESS: &str = r#"
    INSERT INTO course_progress (
        user_id,
        lesson_id,
        completed,
        started_at,
        completed_at,
        last_accessed_at
    )
    VALUES (
        $1,
        $2,
        $3,
        CURRENT_TIMESTAMP,
        CASE WHEN $3 THEN CURRENT_TIMESTAMP ELSE NULL END,
        CURRENT_TIMESTAMP
    )
    ON CONFLICT (user_id, lesson_id) DO UPDATE SET
        completed = EXCLUDED.completed,
        started_at = COALESCE(course_progress.started_at, EXCLUDED.started_at),
        completed_at = CASE
            WHEN EXCLUDED.completed THEN COALESCE(
                course_progress.completed_at,
                EXCLUDED.completed_at
            )
            ELSE NULL
        END,
        last_accessed_at = EXCLUDED.last_accessed_at,
        updated_at = CURRENT_TIMESTAMP
    RETURNING user_id, lesson_id, completed, started_at, completed_at,
              last_accessed_at, updated_at
"#;

/// Returns the published course modules in display order.
pub async fn modules(pool: &PgPool) -> Result<Vec<CourseModule>> {
    sqlx::query_as::<_, CourseModule>(PUBLISHED_MODULES)
        .fetch_all(pool)
        .await
}

/// Explicit course-module query for the learning API.
pub async fn course_modules(pool: &PgPool) -> Result<Vec<CourseModule>> {
    modules(pool).await
}

pub async fn module_by_id(pool: &PgPool, module_id: Uuid) -> Result<Option<CourseModule>> {
    sqlx::query_as::<_, CourseModule>(MODULE_BY_ID)
        .bind(module_id)
        .fetch_optional(pool)
        .await
}

pub async fn module_by_slug(pool: &PgPool, slug: &str) -> Result<Option<CourseModule>> {
    sqlx::query_as::<_, CourseModule>(MODULE_BY_SLUG)
        .bind(slug)
        .fetch_optional(pool)
        .await
}

/// Returns the published lessons in one module in lesson order.
pub async fn lessons_by_module_id(pool: &PgPool, module_id: Uuid) -> Result<Vec<CourseLesson>> {
    sqlx::query_as::<_, CourseLesson>(PUBLISHED_LESSONS_BY_MODULE_ID)
        .bind(module_id)
        .fetch_all(pool)
        .await
}

/// Explicit course-lesson query for one published module.
pub async fn course_lessons(pool: &PgPool, module_id: Uuid) -> Result<Vec<CourseLesson>> {
    lessons_by_module_id(pool, module_id).await
}

pub async fn lesson_by_id(pool: &PgPool, lesson_id: Uuid) -> Result<Option<CourseLesson>> {
    sqlx::query_as::<_, CourseLesson>(LESSON_BY_ID)
        .bind(lesson_id)
        .fetch_optional(pool)
        .await
}

pub async fn lesson_by_module_and_slug(
    pool: &PgPool,
    module_id: Uuid,
    slug: &str,
) -> Result<Option<CourseLesson>> {
    sqlx::query_as::<_, CourseLesson>(LESSON_BY_MODULE_AND_SLUG)
        .bind(module_id)
        .bind(slug)
        .fetch_optional(pool)
        .await
}

pub async fn progress_by_user_id(pool: &PgPool, user_id: Uuid) -> Result<Vec<CourseProgress>> {
    sqlx::query_as::<_, CourseProgress>(PROGRESS_BY_USER_ID)
        .bind(user_id)
        .fetch_all(pool)
        .await
}

/// Explicit user-progress query, newest activity first.
pub async fn user_progress(pool: &PgPool, user_id: Uuid) -> Result<Vec<CourseProgress>> {
    progress_by_user_id(pool, user_id).await
}

pub async fn progress_by_user_and_lesson(
    pool: &PgPool,
    user_id: Uuid,
    lesson_id: Uuid,
) -> Result<Option<CourseProgress>> {
    sqlx::query_as::<_, CourseProgress>(PROGRESS_BY_USER_AND_LESSON)
        .bind(user_id)
        .bind(lesson_id)
        .fetch_optional(pool)
        .await
}

/// Returns completion for one published module, if the module exists.
pub async fn module_completion(
    pool: &PgPool,
    user_id: Uuid,
    module_id: Uuid,
) -> Result<Option<ModuleCompletion>> {
    sqlx::query_as::<_, ModuleCompletion>(MODULE_COMPLETION_BY_USER)
        .bind(user_id)
        .bind(module_id)
        .fetch_optional(pool)
        .await
}

/// Returns aggregate completion across all published course modules.
pub async fn course_completion(pool: &PgPool, user_id: Uuid) -> Result<CourseCompletion> {
    sqlx::query_as::<_, CourseCompletion>(COURSE_COMPLETION_BY_USER)
        .bind(user_id)
        .fetch_one(pool)
        .await
}

/// Records an access and optionally marks the lesson complete.
pub async fn upsert_progress(
    pool: &PgPool,
    user_id: Uuid,
    lesson_id: Uuid,
    completed: bool,
) -> Result<CourseProgress> {
    sqlx::query_as::<_, CourseProgress>(UPSERT_PROGRESS)
        .bind(user_id)
        .bind(lesson_id)
        .bind(completed)
        .fetch_one(pool)
        .await
}

/// Convenience wrapper for callers that only need to complete a lesson.
pub async fn mark_lesson_complete(
    pool: &PgPool,
    user_id: Uuid,
    lesson_id: Uuid,
) -> Result<CourseProgress> {
    upsert_progress(pool, user_id, lesson_id, true).await
}

/// Marks a lesson complete and records the completion/access timestamps.
pub async fn complete_lesson(
    pool: &PgPool,
    user_id: Uuid,
    lesson_id: Uuid,
) -> Result<CourseProgress> {
    mark_lesson_complete(pool, user_id, lesson_id).await
}
