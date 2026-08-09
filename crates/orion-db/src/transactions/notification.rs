use sqlx::PgPool;

use crate::{
    error::DatabaseError,
    models::{NewNotification, Notification},
    queries::notifications,
};

/// Creates or retrieves one deduplicated notification atomically. The
/// transaction boundary is explicit so later outbox insertion can be added
/// without changing callers.
pub async fn create_notification(
    pool: &PgPool,
    notification: NewNotification<'_>,
) -> Result<Notification, DatabaseError> {
    let mut transaction = pool.begin().await.map_err(DatabaseError::from_sqlx)?;
    let result = sqlx::query_as::<_, Notification>(
        r#"
        INSERT INTO notifications (
            user_id, kind, title, body, action_url, deduplication_key, expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id, deduplication_key) DO UPDATE
        SET deduplication_key = notifications.deduplication_key
        RETURNING id, user_id, kind, title, body, action_url,
                  deduplication_key, read_at, expires_at, created_at
        "#,
    )
    .bind(notification.user_id)
    .bind(notification.kind)
    .bind(notification.title)
    .bind(notification.body)
    .bind(notification.action_url)
    .bind(notification.deduplication_key)
    .bind(notification.expires_at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(DatabaseError::from_sqlx)?;
    transaction
        .commit()
        .await
        .map_err(DatabaseError::from_sqlx)?;
    Ok(result)
}

pub async fn list_notifications(
    pool: &PgPool,
    user_id: uuid::Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Notification>, DatabaseError> {
    notifications::list_for_user(pool, user_id, limit, offset)
        .await
        .map_err(DatabaseError::from_sqlx)
}

pub async fn mark_notification_read(
    pool: &PgPool,
    user_id: uuid::Uuid,
    notification_id: uuid::Uuid,
) -> Result<Option<Notification>, DatabaseError> {
    notifications::mark_read(pool, user_id, notification_id)
        .await
        .map_err(DatabaseError::from_sqlx)
}

pub async fn mark_notification_unread(
    pool: &PgPool,
    user_id: uuid::Uuid,
    notification_id: uuid::Uuid,
) -> Result<Option<Notification>, DatabaseError> {
    notifications::mark_unread(pool, user_id, notification_id)
        .await
        .map_err(DatabaseError::from_sqlx)
}

pub async fn unread_notification_count(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<i64, DatabaseError> {
    notifications::unread_count(pool, user_id)
        .await
        .map_err(DatabaseError::from_sqlx)
}
