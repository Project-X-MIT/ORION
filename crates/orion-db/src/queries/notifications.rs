use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{NewNotification, Notification};

const NOTIFICATION_COLUMNS: &str = r#"
    id, user_id, kind, title, body, action_url, deduplication_key,
    read_at, expires_at, created_at
"#;

pub async fn create(pool: &PgPool, notification: NewNotification<'_>) -> Result<Notification> {
    let query = format!(
        r#"
        INSERT INTO notifications (
            user_id, kind, title, body, action_url, deduplication_key, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id, deduplication_key) DO UPDATE
        SET deduplication_key = notifications.deduplication_key
        RETURNING {NOTIFICATION_COLUMNS}
        "#
    );
    sqlx::query_as::<_, Notification>(&query)
        .bind(notification.user_id)
        .bind(notification.kind)
        .bind(notification.title)
        .bind(notification.body)
        .bind(notification.action_url)
        .bind(notification.deduplication_key)
        .bind(notification.expires_at)
        .fetch_one(pool)
        .await
}

pub async fn list_for_user(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Notification>> {
    let query = format!(
        r#"
        SELECT {NOTIFICATION_COLUMNS}
        FROM notifications
        WHERE user_id = $1
          AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
        ORDER BY created_at DESC, id DESC
        LIMIT $2 OFFSET $3
        "#
    );
    sqlx::query_as::<_, Notification>(&query)
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Idempotently marks a notification read. Repeated calls retain the original
/// read timestamp and return the same row.
pub async fn mark_read(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<Option<Notification>> {
    let query = format!(
        r#"
        UPDATE notifications
        SET read_at = COALESCE(read_at, CURRENT_TIMESTAMP)
        WHERE id = $1 AND user_id = $2
        RETURNING {NOTIFICATION_COLUMNS}
        "#
    );
    sqlx::query_as::<_, Notification>(&query)
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn mark_unread(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<Option<Notification>> {
    let query = format!(
        r#"
        UPDATE notifications
        SET read_at = NULL
        WHERE id = $1 AND user_id = $2
        RETURNING {NOTIFICATION_COLUMNS}
        "#
    );
    sqlx::query_as::<_, Notification>(&query)
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn unread_count(pool: &PgPool, user_id: Uuid) -> Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM notifications
        WHERE user_id = $1
          AND read_at IS NULL
          AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
}
