use orion_domain::{
    events::EventEnvelope, ContractError, NotificationKind, NotificationRequestedV1, VersionedEvent,
};
use sqlx::{Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::models::Notification;

/// Errors returned when a consumer cannot safely claim an event.
#[derive(Debug, Error)]
pub enum EventConsumerError {
    #[error("event contract is not supported")]
    Contract(#[from] ContractError),
    #[error("event consumer key cannot be empty")]
    EmptyConsumerKey,
    #[error("event id {event_id} was already claimed with different contract metadata")]
    ConflictingEventIdentity { event_id: Uuid },
    #[error("event consumer database operation failed")]
    Database(#[from] sqlx::Error),
}

/// Claims a validated, typed event for one named consumer. A duplicate
/// delivery returns `Ok(false)` after verifying its contract metadata,
/// allowing the caller to acknowledge the retry without repeating the side
/// effect.
pub async fn claim_versioned_event<T: VersionedEvent>(
    transaction: &mut Transaction<'_, Postgres>,
    consumer_key: &str,
    envelope: &EventEnvelope<T>,
) -> Result<bool, EventConsumerError> {
    envelope.validate_contract()?;
    claim_event(
        transaction,
        consumer_key,
        envelope.event_id.into_uuid(),
        T::EVENT_TYPE,
        T::SCHEMA_VERSION,
    )
    .await
}

async fn claim_event(
    transaction: &mut Transaction<'_, Postgres>,
    consumer_key: &str,
    event_id: Uuid,
    event_type: &str,
    schema_version: u16,
) -> Result<bool, EventConsumerError> {
    if consumer_key.trim().is_empty() {
        return Err(EventConsumerError::EmptyConsumerKey);
    }

    let claimed = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO event_consumptions
            (consumer_key, event_id, event_type, schema_version)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (consumer_key, event_id) DO NOTHING
         RETURNING event_id",
    )
    .bind(consumer_key)
    .bind(event_id)
    .bind(event_type)
    .bind(i32::from(schema_version))
    .fetch_optional(&mut **transaction)
    .await?;

    if claimed.is_some() {
        return Ok(true);
    }

    let existing = sqlx::query_as::<_, (String, i32)>(
        "SELECT event_type, schema_version
         FROM event_consumptions
         WHERE consumer_key = $1 AND event_id = $2",
    )
    .bind(consumer_key)
    .bind(event_id)
    .fetch_one(&mut **transaction)
    .await?;
    if existing.0 != event_type || existing.1 != i32::from(schema_version) {
        return Err(EventConsumerError::ConflictingEventIdentity { event_id });
    }

    Ok(false)
}

/// Consumes a notification request exactly once for one consumer identity.
/// The inbox claim and notification upsert share one transaction: if the
/// effect fails, the claim rolls back and the delivery can be retried safely.
pub async fn consume_notification_requested(
    pool: &sqlx::PgPool,
    envelope: &EventEnvelope<NotificationRequestedV1>,
    consumer_key: &str,
) -> Result<Option<Notification>, EventConsumerError> {
    let mut transaction = pool.begin().await?;
    let claimed = claim_versioned_event(&mut transaction, consumer_key, envelope).await?;
    if !claimed {
        transaction.commit().await?;
        return Ok(None);
    }

    let payload = &envelope.payload;
    let notification = sqlx::query_as::<_, Notification>(
        "INSERT INTO notifications (
            id, user_id, kind, title, body, action_url, deduplication_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id, deduplication_key) DO UPDATE
        SET deduplication_key = notifications.deduplication_key
        RETURNING id, user_id, kind, title, body, action_url,
                  deduplication_key, read_at, expires_at, created_at",
    )
    .bind(payload.notification_id.into_uuid())
    .bind(payload.recipient_id.into_uuid())
    .bind(notification_kind(payload.kind))
    .bind(&payload.title)
    .bind(&payload.body)
    .bind(&payload.action_url)
    .bind(&payload.deduplication_key)
    .fetch_one(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(Some(notification))
}

const fn notification_kind(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::RatingChanged => "rating_changed",
        NotificationKind::ResearchDecision => "research_decision",
        NotificationKind::LearningProgress => "learning_progress",
        NotificationKind::System => "system",
    }
}
