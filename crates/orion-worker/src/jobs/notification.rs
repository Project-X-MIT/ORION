use orion_db::{models::Notification, transactions::consume_notification_requested};
use orion_domain::{EventEnvelope, NotificationRequestedV1};
use orion_redis::{PubSubEnvelope, RedisPublisher};
use sqlx::PgPool;

/// Persists before emitting a best-effort real-time hint. Redis failure never
/// changes the authoritative notification result.
pub async fn process_notification(
    pool: &PgPool,
    publisher: Option<&RedisPublisher>,
    event: &EventEnvelope<NotificationRequestedV1>,
) -> Result<Option<Notification>, orion_db::transactions::EventConsumerError> {
    let notification = consume_notification_requested(pool, event, "notification-worker").await?;
    if let (Some(notification), Some(publisher)) = (&notification, publisher) {
        let hint = PubSubEnvelope {
            event_id: event.event_id.into_uuid(),
            event_type: event.event_type.clone(),
            schema_version: event.schema_version,
            payload: serde_json::json!({
                "notification_id": notification.id,
                "recipient_id": notification.user_id,
            }),
        };
        let _ = publisher
            .publish("orion:v1:pubsub:notification", &hint)
            .await;
    }
    Ok(notification)
}
