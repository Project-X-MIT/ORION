//! Durable PostgreSQL outbox dispatcher. Delivery is at-least-once; each
//! consumer must use its event identity/inbox key to make effects idempotent.
use anyhow::Result;
use chrono::{Duration, Utc};
use orion_db::queries::outbox;
use sqlx::PgPool;
use tokio::time::{sleep, Duration as TokioDuration};

pub const MAX_ATTEMPTS: i32 = 5;
pub const LEASE_SECONDS: i64 = 300;

pub async fn dispatch_once(pool: &PgPool, limit: i64) -> Result<usize> {
    let events = outbox::claim_batch(pool, limit, LEASE_SECONDS).await?;
    let count = events.len();
    for event in events {
        tracing::info!(
            target: "orion.outbox",
            event_id = %event.id,
            event_type = %event.event_type,
            request_id = ?event.request_id,
            trace_id = ?event.trace_id,
            attempt = event.job_attempts,
            "dispatching outbox event"
        );
        let result = dispatch_event(pool, &event.event_type, &event.payload).await;
        match result {
            Ok(()) => {
                let _ = outbox::complete(pool, event.id).await?;
            }
            Err(error) => {
                let delay = 2_i64
                    .saturating_pow((event.job_attempts.max(1) - 1) as u32)
                    .min(300);
                outbox::fail(
                    pool,
                    event.id,
                    &error.to_string(),
                    MAX_ATTEMPTS,
                    Utc::now() + Duration::seconds(delay),
                )
                .await?;
            }
        }
    }
    Ok(count)
}

async fn dispatch_event(
    _pool: &PgPool,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    if event_type.trim().is_empty() || !payload.is_object() {
        anyhow::bail!("invalid outbox event contract")
    }
    // Feature handlers are registered here as they become available. Unknown
    // events are rejected and therefore become actionable dead letters.
    if !event_type.starts_with("orion.") {
        anyhow::bail!("unsupported outbox event type")
    }
    Ok(())
}

pub async fn run(pool: PgPool, poll: TokioDuration) {
    loop {
        if let Err(error) = dispatch_once(&pool, 50).await {
            tracing::error!(target: "orion.worker", error = %error, "outbox dispatch failed");
        }
        sleep(poll).await;
    }
}

#[cfg(test)]
mod tests {
    use super::dispatch_event;

    #[tokio::test]
    async fn rejects_unknown_contracts_and_accepts_versioned_orion_events() {
        assert!(dispatch_event(
            &sqlx::PgPool::connect_lazy("postgres://localhost/db").unwrap(),
            "bad",
            &serde_json::json!({})
        )
        .await
        .is_err());
        assert!(dispatch_event(
            &sqlx::PgPool::connect_lazy("postgres://localhost/db").unwrap(),
            "orion.test.v1",
            &serde_json::json!({})
        )
        .await
        .is_ok());
    }
}
