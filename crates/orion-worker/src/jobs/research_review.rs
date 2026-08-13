use anyhow::{anyhow, Result};
use orion_db::queries::research;
use orion_redis::cache::research::{self as research_cache, ResearchCacheInvalidationEvent};
use orion_redis::RedisClient;
use serde::{Deserialize, Serialize};
use sqlx::{
    types::chrono::{DateTime, Utc},
    PgPool,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use uuid::Uuid;

const RESEARCH_ELO_AWARD_EVENT_TYPE: &str = "orion.research.elo_award.requested";
pub const MAX_RESEARCH_REVIEW_JOB_ATTEMPTS: i32 = 3;
pub const MAX_RESEARCH_REVIEW_ERROR_CHARS: usize = 2_000;
const RESEARCH_REVIEW_RETRY_BACKOFF_SECONDS: [i64; 2] = [30, 300];

const METRIC_ENQUEUED_TOTAL: &str = "orion_research_review_jobs_enqueued_total";
const METRIC_CLAIMED_TOTAL: &str = "orion_research_review_jobs_claimed_total";
const METRIC_COMPLETED_TOTAL: &str = "orion_research_review_jobs_completed_total";
const METRIC_FAILURES_TOTAL: &str = "orion_research_review_jobs_failures_total";
const METRIC_RETRIES_TOTAL: &str = "orion_research_review_jobs_retries_total";
const METRIC_DEAD_LETTER_TOTAL: &str = "orion_research_review_jobs_dead_letter_total";
const METRIC_DURATION_MS_TOTAL: &str = "orion_research_review_job_duration_ms_total";
const METRIC_DURATION_MS_COUNT: &str = "orion_research_review_job_duration_ms_count";
const METRIC_DURATION_MS_MAX: &str = "orion_research_review_job_duration_ms_max";

/// Metric names exposed by [`ResearchReviewJobMetricsSnapshot`].
pub const RESEARCH_REVIEW_METRIC_NAMES: &[&str] = &[
    METRIC_ENQUEUED_TOTAL,
    METRIC_CLAIMED_TOTAL,
    METRIC_COMPLETED_TOTAL,
    METRIC_FAILURES_TOTAL,
    METRIC_RETRIES_TOTAL,
    METRIC_DEAD_LETTER_TOTAL,
    METRIC_DURATION_MS_TOTAL,
    METRIC_DURATION_MS_COUNT,
    METRIC_DURATION_MS_MAX,
];

/// Process-local counters for the research review worker. The worker host can
/// export the snapshot to its metrics backend without exposing paper content,
/// user identifiers, or error payloads.
#[derive(Debug)]
pub struct ResearchReviewJobMetrics {
    enqueued_total: AtomicU64,
    claimed_total: AtomicU64,
    completed_total: AtomicU64,
    failures_total: AtomicU64,
    retries_total: AtomicU64,
    dead_letter_total: AtomicU64,
    duration_ms_total: AtomicU64,
    duration_ms_count: AtomicU64,
    duration_ms_max: AtomicU64,
}

impl Default for ResearchReviewJobMetrics {
    fn default() -> Self {
        Self {
            enqueued_total: AtomicU64::new(0),
            claimed_total: AtomicU64::new(0),
            completed_total: AtomicU64::new(0),
            failures_total: AtomicU64::new(0),
            retries_total: AtomicU64::new(0),
            dead_letter_total: AtomicU64::new(0),
            duration_ms_total: AtomicU64::new(0),
            duration_ms_count: AtomicU64::new(0),
            duration_ms_max: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ResearchReviewJobMetricsSnapshot {
    pub enqueued_total: u64,
    pub claimed_total: u64,
    pub completed_total: u64,
    pub failures_total: u64,
    pub retries_total: u64,
    pub dead_letter_total: u64,
    pub duration_ms_total: u64,
    pub duration_ms_count: u64,
    pub duration_ms_max: u64,
}

static RESEARCH_REVIEW_JOB_METRICS: OnceLock<ResearchReviewJobMetrics> = OnceLock::new();

/// Returns the process-local research review metrics registry.
#[must_use]
pub fn research_review_job_metrics() -> &'static ResearchReviewJobMetrics {
    RESEARCH_REVIEW_JOB_METRICS.get_or_init(ResearchReviewJobMetrics::default)
}

impl ResearchReviewJobMetrics {
    #[must_use]
    pub fn snapshot(&self) -> ResearchReviewJobMetricsSnapshot {
        ResearchReviewJobMetricsSnapshot {
            enqueued_total: self.enqueued_total.load(Ordering::Relaxed),
            claimed_total: self.claimed_total.load(Ordering::Relaxed),
            completed_total: self.completed_total.load(Ordering::Relaxed),
            failures_total: self.failures_total.load(Ordering::Relaxed),
            retries_total: self.retries_total.load(Ordering::Relaxed),
            dead_letter_total: self.dead_letter_total.load(Ordering::Relaxed),
            duration_ms_total: self.duration_ms_total.load(Ordering::Relaxed),
            duration_ms_count: self.duration_ms_count.load(Ordering::Relaxed),
            duration_ms_max: self.duration_ms_max.load(Ordering::Relaxed),
        }
    }

    fn record_enqueued(&self) {
        self.enqueued_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_claimed(&self) {
        self.claimed_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_completed(&self, started_at: Option<DateTime<Utc>>) {
        self.completed_total.fetch_add(1, Ordering::Relaxed);
        self.record_duration(started_at);
    }

    fn record_retry(&self, started_at: Option<DateTime<Utc>>) {
        self.failures_total.fetch_add(1, Ordering::Relaxed);
        self.retries_total.fetch_add(1, Ordering::Relaxed);
        self.record_duration(started_at);
    }

    fn record_dead_letter(&self, started_at: Option<DateTime<Utc>>) {
        self.failures_total.fetch_add(1, Ordering::Relaxed);
        self.dead_letter_total.fetch_add(1, Ordering::Relaxed);
        self.record_duration(started_at);
    }

    fn record_duration(&self, started_at: Option<DateTime<Utc>>) {
        let Some(started_at) = started_at else {
            return;
        };
        let duration_ms = Utc::now()
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0) as u64;
        self.duration_ms_total
            .fetch_add(duration_ms, Ordering::Relaxed);
        self.duration_ms_count.fetch_add(1, Ordering::Relaxed);

        let mut current_max = self.duration_ms_max.load(Ordering::Relaxed);
        while duration_ms > current_max {
            match self.duration_ms_max.compare_exchange_weak(
                current_max,
                duration_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed_max) => current_max = observed_max,
            }
        }
    }
}

/// Durable execution states for the Phantom review handoff job. Transport
/// delivery remains represented by the outbox `status` column; these fields
/// describe the worker's own retryable execution lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchReviewJobState {
    Queued,
    Running,
    Completed,
    Retry,
    DeadLetter,
}

impl ResearchReviewJobState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Retry => "retry",
            Self::DeadLetter => "dead_letter",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "retry" => Ok(Self::Retry),
            "dead_letter" => Ok(Self::DeadLetter),
            _ => Err(anyhow!("unsupported research review job state: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReviewJobMetadata {
    pub event_id: Uuid,
    pub state: ResearchReviewJobState,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub last_failed_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub dead_letter_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Thin worker adapter around the database-owned research award transaction.
///
/// Eligibility, persisted evaluation validation, idempotency, and outbox
/// serialization remain in `orion-db`; this module only records worker
/// lifecycle metrics around the operation.
#[tracing::instrument(
    name = "research_review.enqueue",
    skip(pool, paper_id),
    fields(job = "research_review", event_type = RESEARCH_ELO_AWARD_EVENT_TYPE)
)]
pub async fn enqueue_research_award(pool: &PgPool, paper_id: Uuid) -> Result<bool> {
    let enqueued = research::enqueue_research_award(pool, paper_id).await?;
    if enqueued {
        research_review_job_metrics().record_enqueued();
        tracing::info!(
            target: "orion.worker",
            event = "research_review.enqueued",
            metric = METRIC_ENQUEUED_TOTAL,
            "research review job enqueued"
        );
    }
    Ok(enqueued)
}

/// Compatibility entrypoint for the research award job. The job now queues
/// the request; it does not calculate or apply an Elo award.
#[tracing::instrument(
    name = "research_review.process",
    skip(pool, paper_id),
    fields(job = "research_review", event_type = RESEARCH_ELO_AWARD_EVENT_TYPE)
)]
pub async fn process_research_award(pool: &PgPool, paper_id: Uuid) -> Result<bool> {
    let enqueued = enqueue_research_award(pool, paper_id).await?;
    let Some(metadata) = research_review_job_for_paper(pool, paper_id).await? else {
        return Ok(enqueued);
    };

    if matches!(
        metadata.state,
        ResearchReviewJobState::Queued | ResearchReviewJobState::Retry
    ) {
        if let Some(running) = claim_research_review_job(pool, metadata.event_id).await? {
            tracing::info!(
                event_id = %running.event_id,
                paper_id = %paper_id,
                attempt = running.attempts,
                state = running.state.as_str(),
                "research review handoff job running"
            );
            complete_research_review_job(pool, running.event_id).await?;
            tracing::info!(
                event_id = %running.event_id,
                paper_id = %paper_id,
                state = ResearchReviewJobState::Completed.as_str(),
                "research review handoff job completed"
            );
        }
    }
    Ok(enqueued)
}

/// Invalidates the disposable public-read cache after a publication or
/// published-version change. The database transaction must commit before the
/// caller invokes this hook; a Redis failure never changes PostgreSQL state.
pub async fn invalidate_published_research_cache(
    redis: &RedisClient,
    paper_id: Uuid,
) -> Result<(), research_cache::ResearchCacheError> {
    research_cache::invalidate_after_publication(redis, paper_id).await
}

/// Consumes a committed publication or withdrawal policy event. The operation
/// is deliberately a cache-only side effect: deleting the same key more than
/// once is safe, and a Redis failure can be retried without changing the
/// authoritative research row.
pub async fn invalidate_research_cache_after_policy_event(
    redis: &RedisClient,
    paper_id: Uuid,
    event: ResearchCacheInvalidationEvent,
) -> Result<(), research_cache::ResearchCacheError> {
    research_cache::invalidate_after_policy_event(redis, paper_id, event).await
}

type JobRow = (
    Uuid,
    String,
    i32,
    Option<String>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<String>,
    DateTime<Utc>,
);

fn metadata_from_row(row: JobRow) -> Result<ResearchReviewJobMetadata> {
    Ok(ResearchReviewJobMetadata {
        event_id: row.0,
        state: ResearchReviewJobState::parse(&row.1)?,
        attempts: row.2,
        last_error: row.3,
        last_failed_at: row.4,
        next_retry_at: row.5,
        started_at: row.6,
        completed_at: row.7,
        dead_lettered_at: row.8,
        dead_letter_reason: row.9,
        updated_at: row.10,
    })
}

/// Reads the durable lifecycle metadata for one Phantom handoff event.
pub async fn research_review_job(
    pool: &PgPool,
    event_id: Uuid,
) -> Result<Option<ResearchReviewJobMetadata>> {
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, job_status, job_attempts, job_error,
                job_last_failed_at, job_next_retry_at, job_started_at,
                job_completed_at, job_dead_lettered_at, job_dead_letter_reason,
                job_updated_at
         FROM outbox_events
         WHERE id = $1 AND event_type = $2",
    )
    .bind(event_id)
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .fetch_optional(pool)
    .await?;
    row.map(metadata_from_row).transpose()
}

/// Returns the latest handoff job for a paper, which is useful for operational
/// status endpoints without exposing the internal Elo payload.
pub async fn research_review_job_for_paper(
    pool: &PgPool,
    paper_id: Uuid,
) -> Result<Option<ResearchReviewJobMetadata>> {
    let event_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id
         FROM outbox_events
         WHERE event_type = $1 AND payload ->> 'paper_id' = $2
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .bind(paper_id.to_string())
    .fetch_optional(pool)
    .await?;
    let Some(event_id) = event_id else {
        return Ok(None);
    };
    research_review_job(pool, event_id).await
}

/// Claims one queued/retryable handoff. The row lock and conditional update
/// make concurrent workers converge on one running attempt.
#[tracing::instrument(
    name = "research_review.claim",
    skip(pool),
    fields(job = "research_review", event_type = RESEARCH_ELO_AWARD_EVENT_TYPE)
)]
pub async fn claim_research_review_job(
    pool: &PgPool,
    event_id: Uuid,
) -> Result<Option<ResearchReviewJobMetadata>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, JobRow>(
        "SELECT id, job_status, job_attempts, job_error,
                job_last_failed_at, job_next_retry_at, job_started_at,
                job_completed_at, job_dead_lettered_at, job_dead_letter_reason,
                job_updated_at
         FROM outbox_events
         WHERE id = $1 AND event_type = $2
         FOR UPDATE",
    )
    .bind(event_id)
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        transaction.commit().await?;
        return Ok(None);
    };
    let current = metadata_from_row(row)?;
    if matches!(
        current.state,
        ResearchReviewJobState::Running
            | ResearchReviewJobState::Completed
            | ResearchReviewJobState::DeadLetter
    ) {
        transaction.commit().await?;
        return Ok(None);
    }
    if current.attempts >= MAX_RESEARCH_REVIEW_JOB_ATTEMPTS {
        let dead = sqlx::query_as::<_, JobRow>(
            "UPDATE outbox_events
             SET job_status = 'dead_letter',
                 job_error = COALESCE(job_error, 'maximum retry attempts exceeded'),
                 job_next_retry_at = NULL,
                 job_dead_lettered_at = COALESCE(job_dead_lettered_at, CURRENT_TIMESTAMP),
                 job_dead_letter_reason = COALESCE(
                     job_dead_letter_reason,
                     CONCAT('retry_budget_exhausted_after_', job_attempts, '_attempts')
                 ),
                 job_updated_at = CURRENT_TIMESTAMP
             WHERE id = $1
             RETURNING id, job_status, job_attempts, job_error,
                       job_last_failed_at, job_next_retry_at, job_started_at,
                       job_completed_at, job_dead_lettered_at, job_dead_letter_reason,
                       job_updated_at",
        )
        .bind(event_id)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let metadata = metadata_from_row(dead)?;
        research_review_job_metrics().record_dead_letter(metadata.started_at);
        tracing::error!(
            target: "orion.alerts",
            alert = "research_review_dead_letter",
            severity = "critical",
            event_id = %metadata.event_id,
            attempts = metadata.attempts,
            max_attempts = MAX_RESEARCH_REVIEW_JOB_ATTEMPTS,
            reason = metadata.dead_letter_reason.as_deref().unwrap_or("retry_budget_exhausted"),
            metric = METRIC_DEAD_LETTER_TOTAL,
            "research review job moved to dead letter"
        );
        return Ok(Some(metadata));
    }

    let running = sqlx::query_as::<_, JobRow>(
        "UPDATE outbox_events
         SET job_status = 'running',
             job_attempts = job_attempts + 1,
             job_error = NULL,
             job_next_retry_at = NULL,
             job_started_at = CURRENT_TIMESTAMP,
             job_updated_at = CURRENT_TIMESTAMP
         WHERE id = $1
           AND job_status IN ('queued', 'retry')
           AND (
               job_status = 'queued'
               OR job_next_retry_at IS NULL
               OR job_next_retry_at <= CURRENT_TIMESTAMP
           )
         RETURNING id, job_status, job_attempts, job_error,
                   job_last_failed_at, job_next_retry_at, job_started_at,
                   job_completed_at, job_dead_lettered_at, job_dead_letter_reason,
                   job_updated_at",
    )
    .bind(event_id)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let metadata = running.map(metadata_from_row).transpose()?;
    if let Some(metadata) = metadata.as_ref() {
        research_review_job_metrics().record_claimed();
        tracing::info!(
            target: "orion.worker",
            event = "research_review.running",
            attempt = metadata.attempts,
            max_attempts = MAX_RESEARCH_REVIEW_JOB_ATTEMPTS,
            metric = METRIC_CLAIMED_TOTAL,
            "research review job claimed"
        );
    }
    Ok(metadata)
}

/// Marks a claimed handoff complete. Repeated completion is harmless and does
/// not reapply the downstream Elo effect.
#[tracing::instrument(
    name = "research_review.complete",
    skip(pool),
    fields(job = "research_review", event_type = RESEARCH_ELO_AWARD_EVENT_TYPE)
)]
pub async fn complete_research_review_job(pool: &PgPool, event_id: Uuid) -> Result<bool> {
    let started_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        "UPDATE outbox_events
         SET job_status = 'completed',
             job_error = NULL,
             job_next_retry_at = NULL,
             job_dead_lettered_at = NULL,
             job_dead_letter_reason = NULL,
             job_completed_at = COALESCE(job_completed_at, CURRENT_TIMESTAMP),
             job_updated_at = CURRENT_TIMESTAMP
         WHERE id = $1 AND event_type = $2 AND job_status = 'running'
         RETURNING job_started_at",
    )
    .bind(event_id)
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .fetch_optional(pool)
    .await?;
    if let Some(started_at) = started_at {
        research_review_job_metrics().record_completed(Some(started_at));
        tracing::info!(
            target: "orion.worker",
            event = "research_review.completed",
            metric = METRIC_COMPLETED_TOTAL,
            "research review job completed"
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Records a failed attempt and schedules a retry until the bounded retry
/// budget is exhausted, after which the job becomes dead-lettered.
#[tracing::instrument(
    name = "research_review.fail",
    skip(pool, error),
    fields(job = "research_review", event_type = RESEARCH_ELO_AWARD_EVENT_TYPE)
)]
pub async fn fail_research_review_job(
    pool: &PgPool,
    event_id: Uuid,
    error: &str,
) -> Result<Option<ResearchReviewJobMetadata>> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query_as::<_, (i32, String)>(
        "SELECT job_attempts, job_status
         FROM outbox_events
         WHERE id = $1 AND event_type = $2
         FOR UPDATE",
    )
    .bind(event_id)
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((attempts, state)) = row else {
        transaction.commit().await?;
        return Ok(None);
    };
    if state != ResearchReviewJobState::Running.as_str() {
        transaction.commit().await?;
        return research_review_job(pool, event_id).await;
    }
    let next_state = if attempts >= MAX_RESEARCH_REVIEW_JOB_ATTEMPTS {
        ResearchReviewJobState::DeadLetter
    } else {
        ResearchReviewJobState::Retry
    };
    let safe_failure_context = safe_failure_context(error);
    let retry_delay_seconds = retry_backoff_seconds(attempts);
    let dead_letter_reason = (next_state == ResearchReviewJobState::DeadLetter)
        .then(|| dead_letter_reason(attempts, &safe_failure_context));
    let updated = sqlx::query_as::<_, JobRow>(
        "UPDATE outbox_events
         SET job_status = $2,
             job_error = $3,
             job_last_failed_at = CURRENT_TIMESTAMP,
             job_next_retry_at = CASE
                 WHEN $2 = 'retry'
                 THEN CURRENT_TIMESTAMP + ($4::double precision * INTERVAL '1 second')
                 ELSE NULL
             END,
             job_dead_lettered_at = CASE
                 WHEN $2 = 'dead_letter' THEN COALESCE(job_dead_lettered_at, CURRENT_TIMESTAMP)
                 ELSE NULL
             END,
             job_dead_letter_reason = $5,
             job_completed_at = NULL,
             job_updated_at = CURRENT_TIMESTAMP
         WHERE id = $1
         RETURNING id, job_status, job_attempts, job_error,
                   job_last_failed_at, job_next_retry_at, job_started_at,
                   job_completed_at, job_dead_lettered_at, job_dead_letter_reason,
                   job_updated_at",
    )
    .bind(event_id)
    .bind(next_state.as_str())
    .bind(&safe_failure_context)
    .bind(retry_delay_seconds)
    .bind(dead_letter_reason)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let metadata = metadata_from_row(updated)?;
    match metadata.state {
        ResearchReviewJobState::Retry => {
            research_review_job_metrics().record_retry(metadata.started_at);
            tracing::warn!(
                target: "orion.alerts",
                alert = "research_review_retry_scheduled",
                severity = "warning",
                event_id = %metadata.event_id,
                attempt = metadata.attempts,
                max_attempts = MAX_RESEARCH_REVIEW_JOB_ATTEMPTS,
                retry_after_seconds = retry_delay_seconds,
                metric = METRIC_RETRIES_TOTAL,
                "research review job failed and was scheduled for retry"
            );
        }
        ResearchReviewJobState::DeadLetter => {
            research_review_job_metrics().record_dead_letter(metadata.started_at);
            tracing::error!(
                target: "orion.alerts",
                alert = "research_review_dead_letter",
                severity = "critical",
                event_id = %metadata.event_id,
                attempts = metadata.attempts,
                max_attempts = MAX_RESEARCH_REVIEW_JOB_ATTEMPTS,
                reason = metadata.dead_letter_reason.as_deref().unwrap_or("retry_budget_exhausted"),
                metric = METRIC_DEAD_LETTER_TOTAL,
                "research review job exhausted its retry budget"
            );
        }
        _ => {}
    }
    tracing::debug!(
        target: "orion.worker",
        event = "research_review.failed",
        metric = METRIC_FAILURES_TOTAL,
        "research review job attempt failed"
    );
    Ok(Some(metadata))
}

fn retry_backoff_seconds(attempts: i32) -> i64 {
    let index = attempts.saturating_sub(1).max(0) as usize;
    RESEARCH_REVIEW_RETRY_BACKOFF_SECONDS
        .get(index)
        .copied()
        .unwrap_or_else(|| {
            RESEARCH_REVIEW_RETRY_BACKOFF_SECONDS[RESEARCH_REVIEW_RETRY_BACKOFF_SECONDS.len() - 1]
        })
}

/// Converts an arbitrary worker error into a small operational category.
///
/// Raw errors are not safe job metadata: database/serialization errors can
/// contain report content, SQL details, or other private values. Classification
/// is deliberately allowlisted and returns no caller-provided text.
fn safe_failure_context(error: &str) -> String {
    let normalized = error.to_ascii_lowercase();
    let context = if normalized.contains("timeout") || normalized.contains("timed out") {
        "dependency_timeout"
    } else if normalized.contains("redis") {
        "redis_dependency_failed"
    } else if normalized.contains("outbox") {
        "outbox_persistence_failed"
    } else if normalized.contains("database")
        || normalized.contains("postgres")
        || normalized.contains("sqlx")
        || normalized.contains("transaction")
        || normalized.contains("constraint")
    {
        "database_persistence_failed"
    } else if normalized.contains("evaluation")
        || normalized.contains("rubric")
        || normalized.contains("review")
    {
        "invalid_research_evaluation"
    } else if normalized.contains("json")
        || normalized.contains("serializ")
        || normalized.contains("deserializ")
    {
        "payload_serialization_failed"
    } else {
        "worker_execution_failed"
    };

    context
        .chars()
        .take(MAX_RESEARCH_REVIEW_ERROR_CHARS)
        .collect()
}

fn dead_letter_reason(attempts: i32, failure_context: &str) -> String {
    format!("retry_budget_exhausted_after_{attempts}_attempts:{failure_context}")
}

#[cfg(test)]
mod tests {
    use super::{
        dead_letter_reason, retry_backoff_seconds, safe_failure_context, ResearchReviewJobState,
        MAX_RESEARCH_REVIEW_ERROR_CHARS, MAX_RESEARCH_REVIEW_JOB_ATTEMPTS,
        RESEARCH_ELO_AWARD_EVENT_TYPE, RESEARCH_REVIEW_METRIC_NAMES,
    };

    #[test]
    fn job_states_are_stable_wire_values() {
        assert_eq!(ResearchReviewJobState::Queued.as_str(), "queued");
        assert_eq!(ResearchReviewJobState::Running.as_str(), "running");
        assert_eq!(ResearchReviewJobState::Completed.as_str(), "completed");
        assert_eq!(ResearchReviewJobState::Retry.as_str(), "retry");
        assert_eq!(ResearchReviewJobState::DeadLetter.as_str(), "dead_letter");
        for state in [
            ResearchReviewJobState::Queued,
            ResearchReviewJobState::Running,
            ResearchReviewJobState::Completed,
            ResearchReviewJobState::Retry,
            ResearchReviewJobState::DeadLetter,
        ] {
            let encoded = serde_json::to_string(&state).expect("job state serializes");
            let decoded: ResearchReviewJobState =
                serde_json::from_str(&encoded).expect("job state deserializes");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn retry_budget_is_bounded_and_explicit() {
        assert_eq!(MAX_RESEARCH_REVIEW_JOB_ATTEMPTS, 3);
        assert_eq!(retry_backoff_seconds(1), 30);
        assert_eq!(retry_backoff_seconds(2), 300);
        assert_eq!(retry_backoff_seconds(3), 300);
    }

    #[test]
    fn metric_names_are_stable_and_exportable() {
        assert_eq!(RESEARCH_REVIEW_METRIC_NAMES.len(), 9);
        assert!(
            RESEARCH_REVIEW_METRIC_NAMES.contains(&"orion_research_review_jobs_dead_letter_total")
        );
        assert!(RESEARCH_REVIEW_METRIC_NAMES.contains(&"orion_research_review_job_duration_ms_max"));
    }

    #[test]
    fn dead_letter_context_is_bounded_and_actionable_without_raw_error_text() {
        let error = "outbox insert failed for private research title: hidden report";
        assert_eq!(safe_failure_context(error), "outbox_persistence_failed");
        assert_eq!(
            dead_letter_reason(3, "outbox_persistence_failed"),
            "retry_budget_exhausted_after_3_attempts:outbox_persistence_failed"
        );

        let long_error = "x".repeat(MAX_RESEARCH_REVIEW_ERROR_CHARS + 1);
        assert_eq!(safe_failure_context(&long_error), "worker_execution_failed");
        assert_eq!(
            safe_failure_context("persisted research evaluation contains private review text"),
            "invalid_research_evaluation"
        );
        assert_eq!(
            safe_failure_context("database error: report body and reviewer comments"),
            "database_persistence_failed"
        );
    }

    #[test]
    fn uses_the_versioned_research_request_event_type() {
        assert_eq!(
            RESEARCH_ELO_AWARD_EVENT_TYPE,
            "orion.research.elo_award.requested"
        );
    }
}
