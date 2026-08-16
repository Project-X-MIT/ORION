use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Ensures one durable event exists for a deterministic business identity.
/// Returns whether the event still needs its typed post-commit effects.
pub async fn ensure_pending_event(
    pool: &PgPool,
    event_id: Uuid,
    event_type: &str,
    schema_version: i32,
    payload: impl Serialize,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO outbox_events (id, event_type, schema_version, payload)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (id) DO NOTHING
         RETURNING id",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(schema_version)
    .bind(sqlx::types::Json(payload))
    .fetch_optional(pool)
    .await?;

    if inserted.is_some() {
        return Ok(true);
    }

    // This is a new statement/snapshot after a concurrent conflicting insert
    // has committed, so it can observe the existing event reliably.
    sqlx::query_scalar::<_, bool>("SELECT status = 'pending' FROM outbox_events WHERE id = $1")
        .bind(event_id)
        .fetch_one(pool)
        .await
}

pub async fn mark_event_dispatched(pool: &PgPool, event_id: Uuid) -> sqlx::Result<bool> {
    let changed = sqlx::query(
        "UPDATE outbox_events
         SET status = 'dispatched',
             dispatched_at = COALESCE(dispatched_at, CURRENT_TIMESTAMP),
             job_status = 'completed',
             job_completed_at = COALESCE(job_completed_at, CURRENT_TIMESTAMP),
             lease_until = NULL,
             job_updated_at = CURRENT_TIMESTAMP
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(event_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(changed == 1)
}

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
