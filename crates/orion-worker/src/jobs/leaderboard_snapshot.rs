//! Idempotent leaderboard rank-snapshot job body.
//!
//! Div owns registration of the schedule. This module owns the execution
//! contract: an hourly UTC cadence, the immediately preceding completed
//! snapshot as the movement window, and post-commit cache/event effects.

use std::{future::Future, pin::Pin};

use chrono::{DateTime, Duration, Timelike, Utc};
use orion_db::transactions::{ensure_pending_event, mark_event_dispatched, snapshot_leaderboard};
use orion_domain::events::ensure_event_compatible;
use orion_redis::{cache::leaderboard::LeaderboardCache, PubSubEnvelope, RedisPublisher};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

/// Approved production cadence and comparison window.
pub const SNAPSHOT_CADENCE: Duration = Duration::hours(1);
pub const SNAPSHOT_EVENT_TYPE: &str = "orion.leaderboard.snapshot.completed";
pub const SNAPSHOT_EVENT_SCHEMA_VERSION: u16 = 1;
pub const SNAPSHOT_EVENT_CHANNEL: &str = "orion:v1:pubsub:leaderboard";

/// One logical scheduled snapshot. Retries within the same UTC hour resolve to
/// the same timestamp and identity, including after a worker restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotWindow {
    pub snapshot_at: DateTime<Utc>,
    pub snapshot_id: Uuid,
}

impl SnapshotWindow {
    #[must_use]
    pub fn for_scheduled_time(scheduled_for: DateTime<Utc>) -> Self {
        let snapshot_at = scheduled_for
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .expect("UTC hour boundaries are always valid");
        Self {
            snapshot_at,
            snapshot_id: deterministic_snapshot_id(snapshot_at),
        }
    }

    #[must_use]
    pub fn comparison_window_start(self) -> DateTime<Utc> {
        self.snapshot_at - SNAPSHOT_CADENCE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotOutcome {
    pub window: SnapshotWindow,
    pub inserted_rows: u64,
    pub effects_pending: bool,
}

impl SnapshotOutcome {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.inserted_rows > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCompletedV1 {
    pub snapshot_id: Uuid,
    pub snapshot_at: DateTime<Utc>,
    pub comparison_snapshot_before: DateTime<Utc>,
    pub inserted_rows: u64,
}

pub type SnapshotEffectFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), SnapshotJobError>> + Send + 'a>>;

/// Post-commit boundary used by production adapters and fault-ordering tests.
pub trait SnapshotEffects: Send + Sync {
    fn after_commit(&self, outcome: SnapshotOutcome) -> SnapshotEffectFuture<'_>;
}

/// Runs the completed DB-03 transaction. PostgreSQL serializes overlapping
/// writers and the deterministic hour key makes retries one logical snapshot.
pub async fn run_leaderboard_snapshot<E: SnapshotEffects>(
    pool: &PgPool,
    scheduled_for: DateTime<Utc>,
    effects: &E,
) -> Result<SnapshotOutcome, SnapshotJobError> {
    let window = SnapshotWindow::for_scheduled_time(scheduled_for);
    ensure_event_compatible(SNAPSHOT_EVENT_TYPE, SNAPSHOT_EVENT_SCHEMA_VERSION)?;
    let inserted_rows = snapshot_leaderboard(pool, window.snapshot_at).await?;
    let event = SnapshotCompletedV1 {
        snapshot_id: window.snapshot_id,
        snapshot_at: window.snapshot_at,
        comparison_snapshot_before: window.comparison_window_start(),
        inserted_rows,
    };
    let effects_pending = ensure_pending_event(
        pool,
        window.snapshot_id,
        SNAPSHOT_EVENT_TYPE,
        i32::from(SNAPSHOT_EVENT_SCHEMA_VERSION),
        &event,
    )
    .await?;
    let outcome = SnapshotOutcome {
        window,
        inserted_rows,
        effects_pending,
    };

    // DB-03 returns only after commit. No cache or publication can run before
    // this point, and a duplicate/backdated execution produces no effects.
    if outcome.effects_pending {
        effects.after_commit(outcome).await?;
    }
    Ok(outcome)
}

/// Reconciliation uses the same logical identity and transaction as the
/// scheduled path. It is therefore safe after a crash and produces identical
/// ranks when authoritative `user_ratings` is unchanged.
pub async fn rebuild_snapshot<E: SnapshotEffects>(
    pool: &PgPool,
    snapshot_at: DateTime<Utc>,
    effects: &E,
) -> Result<SnapshotOutcome, SnapshotJobError> {
    run_leaderboard_snapshot(pool, snapshot_at, effects).await
}

pub struct RedisSnapshotEffects<'a> {
    pub pool: &'a PgPool,
    pub cache: &'a LeaderboardCache,
    pub publisher: &'a RedisPublisher,
}

impl SnapshotEffects for RedisSnapshotEffects<'_> {
    fn after_commit(&self, outcome: SnapshotOutcome) -> SnapshotEffectFuture<'_> {
        Box::pin(async move {
            self.cache
                // A pending retry is still a committed snapshot effect even
                // when the snapshot insert itself was an idempotent no-op.
                .after_snapshot_commit(outcome.inserted_rows.max(1), &[])
                .await?;
            let event = PubSubEnvelope {
                event_id: outcome.window.snapshot_id,
                event_type: SNAPSHOT_EVENT_TYPE.to_owned(),
                schema_version: SNAPSHOT_EVENT_SCHEMA_VERSION,
                payload: SnapshotCompletedV1 {
                    snapshot_id: outcome.window.snapshot_id,
                    snapshot_at: outcome.window.snapshot_at,
                    comparison_snapshot_before: outcome.window.comparison_window_start(),
                    inserted_rows: outcome.inserted_rows,
                },
            };
            self.publisher
                .publish(SNAPSHOT_EVENT_CHANNEL, &event)
                .await?;
            mark_event_dispatched(self.pool, outcome.window.snapshot_id).await?;
            Ok(())
        })
    }
}

#[derive(Debug, Error)]
pub enum SnapshotJobError {
    #[error("leaderboard snapshot event contract is incompatible")]
    Contract(#[from] orion_domain::ContractError),
    #[error("leaderboard snapshot transaction failed")]
    Database(#[from] sqlx::Error),
    #[error("leaderboard cache invalidation failed after commit")]
    Cache(#[from] orion_redis::cache::leaderboard::LeaderboardCacheError),
    #[error("leaderboard completion publication failed after commit")]
    Publish(#[from] orion_redis::PubSubError),
}

fn deterministic_snapshot_id(snapshot_at: DateTime<Utc>) -> Uuid {
    // UUIDv8 layout with the signed Unix-hour encoded in the low 64 bits and a
    // fixed ORION leaderboard namespace in the high bits. No random state or
    // process-local clock participates in identity generation.
    const NAMESPACE: u128 = 0x4f52_494f_4e4c_4800_0000_0000_0000_0000;
    let hour = snapshot_at
        .timestamp()
        .div_euclid(SNAPSHOT_CADENCE.num_seconds());
    let mut value = NAMESPACE | u128::from(u64::from_be_bytes(hour.to_be_bytes()));
    value = (value & !(0xf_u128 << 76)) | (8_u128 << 76);
    value = (value & !(0x3_u128 << 62)) | (0x2_u128 << 62);
    Uuid::from_u128(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn retries_share_the_hour_identity_and_comparison_window() {
        let first = SnapshotWindow::for_scheduled_time(
            Utc.with_ymd_and_hms(2026, 8, 17, 10, 1, 2).unwrap(),
        );
        let retry = SnapshotWindow::for_scheduled_time(
            Utc.with_ymd_and_hms(2026, 8, 17, 10, 59, 59).unwrap(),
        );

        assert_eq!(first, retry);
        assert_eq!(
            first.comparison_window_start(),
            Utc.with_ymd_and_hms(2026, 8, 17, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn adjacent_windows_have_distinct_deterministic_identities() {
        let first = SnapshotWindow::for_scheduled_time(
            Utc.with_ymd_and_hms(2026, 8, 17, 10, 0, 0).unwrap(),
        );
        let next = SnapshotWindow::for_scheduled_time(
            Utc.with_ymd_and_hms(2026, 8, 17, 11, 0, 0).unwrap(),
        );

        assert_ne!(first.snapshot_id, next.snapshot_id);
        assert_eq!(first.snapshot_id.get_version_num(), 8);
    }

    #[test]
    fn no_op_snapshot_never_requests_post_commit_effects() {
        let outcome = SnapshotOutcome {
            window: SnapshotWindow::for_scheduled_time(Utc.timestamp_opt(0, 0).unwrap()),
            inserted_rows: 0,
            effects_pending: false,
        };
        assert!(!outcome.changed());
    }
}
