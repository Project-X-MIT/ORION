use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Inserts a durable event into the caller's transaction.
///
/// The event payload is intentionally generic. Event contracts and their
/// serialization are owned by `orion-domain`; this module only persists the
/// serialized body and routing type.
pub async fn write_outbox_event(
    transaction: &mut Transaction<'_, Postgres>,
    event_type: &str,
    payload: impl Serialize,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO outbox_events (event_type, payload)
         VALUES ($1, $2)
         RETURNING id",
    )
    .bind(event_type)
    .bind(sqlx::types::Json(payload))
    .fetch_one(&mut **transaction)
    .await
}

pub async fn write_outbox_event_with_context(
    transaction: &mut Transaction<'_, Postgres>,
    event_type: &str,
    schema_version: i32,
    payload: impl Serialize,
    request_id: Option<Uuid>,
    trace_id: Option<&str>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar("INSERT INTO outbox_events (event_type, schema_version, payload, request_id, trace_id) VALUES ($1,$2,$3,$4,$5) RETURNING id")
        .bind(event_type).bind(schema_version).bind(sqlx::types::Json(payload)).bind(request_id).bind(trace_id)
        .fetch_one(&mut **transaction).await
}

// TODO: Add the polling worker or LISTEN/NOTIFY dispatcher in a later phase.
