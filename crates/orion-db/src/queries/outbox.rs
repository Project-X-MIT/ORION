use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::OutboxEvent;

const COLUMNS: &str =
    "e.id, e.event_type, e.payload, e.status, e.schema_version, e.request_id, e.trace_id,
    e.created_at, e.dispatched_at, e.retry_count, e.job_status, e.job_attempts, e.job_error,
    e.job_next_retry_at, e.job_started_at, e.lease_until";

pub async fn claim_batch(
    pool: &PgPool,
    limit: i64,
    lease_seconds: i64,
) -> sqlx::Result<Vec<OutboxEvent>> {
    let query = format!(
        "WITH candidates AS (
            SELECT id FROM outbox_events
            WHERE status = 'pending'
              -- Registered feature jobs own their typed consumer adapters;
              -- the generic dispatcher must never mark them complete.
              AND event_type NOT IN ('orion.research.elo_award.requested', 'orion.notification.requested')
              AND (job_status IN ('queued','retry') AND (job_next_retry_at IS NULL OR job_next_retry_at <= CURRENT_TIMESTAMP)
                   OR job_status = 'running' AND lease_until < CURRENT_TIMESTAMP)
            ORDER BY created_at, id LIMIT $1 FOR UPDATE SKIP LOCKED
        )
        UPDATE outbox_events e SET job_status='running', job_attempts=e.job_attempts+1,
            retry_count=e.retry_count+1, job_error=NULL, job_started_at=CURRENT_TIMESTAMP,
            lease_until=CURRENT_TIMESTAMP + ($2::double precision * INTERVAL '1 second'),
            job_updated_at=CURRENT_TIMESTAMP
        FROM candidates c WHERE e.id=c.id RETURNING {COLUMNS}"
    );
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, OutboxEvent>(&query)
        .bind(limit)
        .bind(lease_seconds)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows)
}

/// Claims only events owned by a typed worker adapter. Typed consumers use
/// this path when the generic dispatcher must not acknowledge an event before
/// its business effect and inbox claim have committed.
pub async fn claim_batch_for_event_type(
    pool: &PgPool,
    event_type: &str,
    limit: i64,
    lease_seconds: i64,
) -> sqlx::Result<Vec<OutboxEvent>> {
    let query = format!(
        "WITH candidates AS (
            SELECT id FROM outbox_events
            WHERE status = 'pending'
              AND event_type = $3
              AND (job_status IN ('queued','retry') AND (job_next_retry_at IS NULL OR job_next_retry_at <= CURRENT_TIMESTAMP)
                   OR job_status = 'running' AND lease_until < CURRENT_TIMESTAMP)
            ORDER BY created_at, id LIMIT $1 FOR UPDATE SKIP LOCKED
        )
        UPDATE outbox_events e SET job_status='running', job_attempts=e.job_attempts+1,
            retry_count=e.retry_count+1, job_error=NULL, job_started_at=CURRENT_TIMESTAMP,
            lease_until=CURRENT_TIMESTAMP + ($2::double precision * INTERVAL '1 second'),
            job_updated_at=CURRENT_TIMESTAMP
        FROM candidates c WHERE e.id=c.id RETURNING {COLUMNS}"
    );
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, OutboxEvent>(&query)
        .bind(limit)
        .bind(lease_seconds)
        .bind(event_type)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn complete(pool: &PgPool, event_id: Uuid) -> sqlx::Result<bool> {
    let changed = sqlx::query("UPDATE outbox_events SET status='dispatched', dispatched_at=COALESCE(dispatched_at,CURRENT_TIMESTAMP), job_status='completed', job_completed_at=COALESCE(job_completed_at,CURRENT_TIMESTAMP), lease_until=NULL, job_updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND job_status='running'")
        .bind(event_id).execute(pool).await?.rows_affected();
    Ok(changed == 1)
}

pub async fn fail(
    pool: &PgPool,
    event_id: Uuid,
    error: &str,
    max_attempts: i32,
    retry_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE outbox_events SET job_status=CASE WHEN job_attempts >= $3 THEN 'dead_letter' ELSE 'retry' END, job_error=left($2,2000), job_next_retry_at=CASE WHEN job_attempts >= $3 THEN NULL ELSE $4 END, job_dead_lettered_at=CASE WHEN job_attempts >= $3 THEN CURRENT_TIMESTAMP ELSE NULL END, job_dead_letter_reason=CASE WHEN job_attempts >= $3 THEN 'retry_budget_exhausted' ELSE NULL END, lease_until=NULL, job_updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND job_status='running'")
        .bind(event_id).bind(error).bind(max_attempts).bind(retry_at).execute(pool).await?;
    Ok(())
}

pub async fn replay(pool: &PgPool, event_id: Uuid) -> sqlx::Result<bool> {
    let changed = sqlx::query("UPDATE outbox_events SET status='pending', job_status='queued', job_next_retry_at=NULL, job_error=NULL, lease_until=NULL, job_dead_lettered_at=NULL, job_dead_letter_reason=NULL, job_updated_at=CURRENT_TIMESTAMP WHERE id=$1 AND job_status='dead_letter'")
        .bind(event_id).execute(pool).await?.rows_affected();
    Ok(changed == 1)
}
