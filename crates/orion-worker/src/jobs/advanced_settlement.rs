use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
};

use chrono::Utc;
use orion_db::{
    models::{
        AdvancedSettlementInput, AdvancedSettlementResolution, QuizAttempt, QuizSettlementResult,
        ATTEMPT_COMPLETED,
    },
    queries::{advanced_actuals, quiz_attempts, quiz_questions, ratings},
    transactions::settle_advanced_actual_quiz,
};
pub use orion_domain::quiz::{AdvancedActualValue, AdvancedPrediction};
use orion_domain::{
    events::ensure_event_compatible, AdvancedSubmissionRequestedV1, VersionedEvent,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;

use crate::scheduler::RetryPolicy;

/// Outbox contracts emitted after an Advanced settlement commits.
pub const ADVANCED_SETTLED_EVENT_TYPE: &str = "orion.quiz.advanced.settled";
pub const ADVANCED_CACHE_INVALIDATION_EVENT_TYPE: &str = "orion.quiz.cache.invalidate";
pub const ADVANCED_NOTIFICATION_EVENT_TYPE: &str = "orion.notification.requested";
pub const ADVANCED_DEAD_LETTER_EVENT_TYPE: &str = "orion.quiz.advanced.settlement.dead_lettered";
pub const ADVANCED_SUBMITTED_EVENT_TYPE: &str = AdvancedSubmissionRequestedV1::EVENT_TYPE;
pub const ADVANCED_PROVIDER_OUTAGE_ALERT: &str = "advanced_actual_provider_unavailable";
pub const ADVANCED_SETTLEMENT_SCHEMA_VERSION: i32 = 1;
pub const MAX_ADVANCED_SETTLEMENT_ATTEMPTS: u32 = 5;

/// Reads final provider facts from PostgreSQL. An external approved ingestion
/// adapter owns writing `advanced_actual_values`; this worker never calls a
/// client-controlled endpoint and never uses Redis as an actual-value source.
#[derive(Debug, Clone)]
pub struct PostgresAdvancedActualProvider {
    pool: PgPool,
}

impl PostgresAdvancedActualProvider {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Minimal immutable ADR question contract supplied by DB-02.
#[derive(Debug, Clone)]
pub struct AdvancedQuestion {
    pub id: Uuid,
    pub value_scale: u32,
    pub horizon_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
    pub provider_key: String,
}

pub type ActualFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ResolvedActual, ActualProviderError>> + Send + 'a>>;

/// The DB-02 adapter supplies the immutable Advanced question contract and
/// prediction. The actual value is fetched from the provider and passed into
/// the atomic DB-02 settlement transaction after validation.
#[derive(Debug, Clone)]
pub struct AdvancedResolution {
    pub question: AdvancedQuestion,
    pub prediction: AdvancedPrediction,
}

#[derive(Debug, Clone)]
pub struct AdvancedAttemptContext {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub resolutions: Vec<AdvancedResolution>,
}

/// Boundaries used by recovery tests and operational fault injection. An
/// injected failure must behave like a process crash: PostgreSQL rolls back
/// any open transaction, while a committed settlement is recoverable from its
/// completed attempt row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedSettlementBoundary {
    AfterPendingLookup,
    AfterActualValidation,
    BeforeAtomicSettlement,
    AfterAtomicSettlement,
    BeforeOutbox,
    AfterOutbox,
}

pub trait AdvancedSettlementHooks: Send + Sync {
    fn reached(&self, boundary: AdvancedSettlementBoundary) -> Result<(), AdvancedSettlementError>;
}

struct NoopSettlementHooks;

impl AdvancedSettlementHooks for NoopSettlementHooks {
    fn reached(
        &self,
        _boundary: AdvancedSettlementBoundary,
    ) -> Result<(), AdvancedSettlementError> {
        Ok(())
    }
}

static ADVANCED_PROVIDER_OUTAGES: OnceLock<AtomicU64> = OnceLock::new();

#[must_use]
pub fn advanced_provider_outage_count() -> u64 {
    ADVANCED_PROVIDER_OUTAGES
        .get_or_init(|| AtomicU64::new(0))
        .load(Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct ResolvedActual {
    pub value: AdvancedActualValue,
}

/// Provider boundary for the approved ADR. Implementations should use a
/// bounded request timeout and return `Unavailable` for a transient outage;
/// the worker never treats an incomplete provider response as a value.
pub trait AdvancedActualProvider: Send + Sync {
    fn obtain_actual<'a>(&'a self, question: &'a AdvancedQuestion) -> ActualFuture<'a>;
}

impl AdvancedActualProvider for PostgresAdvancedActualProvider {
    fn obtain_actual<'a>(&'a self, question: &'a AdvancedQuestion) -> ActualFuture<'a> {
        Box::pin(async move {
            let Some(actual) = advanced_actuals::by_question_id(&self.pool, question.id)
                .await
                .map_err(|_| ActualProviderError::Unavailable)?
            else {
                return Err(ActualProviderError::Unavailable);
            };

            if !actual.is_final {
                return Err(ActualProviderError::Unavailable);
            }

            Ok(ResolvedActual {
                value: AdvancedActualValue {
                    question_id: actual.question_id,
                    value: actual.value,
                    observed_at: actual.observed_at,
                    available_at: actual.available_at,
                    source_id: actual.source_id,
                    source_version: actual.source_version,
                    is_final: actual.is_final,
                },
            })
        })
    }
}

#[derive(Debug, Clone, Error)]
pub enum ActualProviderError {
    #[error("Advanced actual provider is unavailable")]
    Unavailable,
    #[error("Advanced actual provider returned a terminal failure")]
    Terminal,
}

#[derive(Debug, Error)]
pub enum AdvancedValidationError {
    #[error("Advanced question expiry must be after its horizon")]
    InvalidQuestionWindow,
    #[error("Advanced prediction targets the wrong question")]
    PredictionQuestionMismatch,
    #[error("Advanced prediction is after the question horizon")]
    PredictionAfterHorizon,
    #[error("Advanced value exceeds the question's decimal scale")]
    ValueScaleExceeded,
    #[error("Advanced actual value is not final")]
    ActualNotFinal,
    #[error("Advanced actual value was observed after the horizon")]
    ActualObservedAfterHorizon,
    #[error("Advanced actual value is available before the horizon")]
    ActualAvailableBeforeHorizon,
    #[error("Advanced actual value is available after expiry")]
    ActualAvailableAfterExpiry,
    #[error("Advanced actual value is available before it was observed")]
    ActualAvailableBeforeObserved,
    #[error("Advanced actual source metadata is empty")]
    EmptyActualSource,
    #[error("Advanced actual source does not match the question provider contract")]
    ActualSourceMismatch,
}

#[derive(Debug, Error)]
pub enum AdvancedSettlementError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("Advanced actual provider is unavailable")]
    ProviderUnavailable,
    #[error("Advanced actual provider returned a terminal failure")]
    ProviderTerminal,
    #[error("Advanced actual value is not available yet")]
    ActualNotAvailable,
    #[error("Advanced actual value failed ADR validation")]
    InvalidActual(#[from] AdvancedValidationError),
    #[error("pending Advanced attempt context is invalid")]
    InvalidAttemptContext,
    #[error("outbox operation failed")]
    Outbox(#[source] sqlx::Error),
    #[error("injected Advanced settlement crash at {0:?}")]
    InjectedCrash(AdvancedSettlementBoundary),
}

impl AdvancedSettlementError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Database(_)
                | Self::ProviderUnavailable
                | Self::ActualNotAvailable
                | Self::Outbox(_)
                | Self::InjectedCrash(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AdvancedSettlementOutcome {
    Completed(QuizSettlementResult),
    AlreadyCompleted(QuizSettlementResult),
    DeadLettered,
}

/// DB-02 pending-attempt lookup. Keeping this call in the worker makes it
/// impossible for a caller to settle an arbitrary non-Advanced attempt.
pub async fn locate_pending_advanced_attempts(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<QuizAttempt>, sqlx::Error> {
    quiz_attempts::pending_advanced_by_user_id(pool, user_id, limit, offset).await
}

/// Loads the exact numeric predictions persisted by the API for a pending
/// attempt. Question contracts remain supplied by the DB-02 adapter/provider
/// boundary; this function only reads PostgreSQL prediction facts.
pub async fn load_advanced_predictions(
    pool: &PgPool,
    attempt_id: Uuid,
) -> Result<Vec<AdvancedPrediction>, sqlx::Error> {
    quiz_attempts::advanced_predictions_by_attempt_id(pool, attempt_id)
        .await
        .map(|predictions| {
            predictions
                .into_iter()
                .map(|prediction| AdvancedPrediction {
                    question_id: prediction.question_id,
                    value: prediction.value,
                    submitted_at: prediction.submitted_at,
                })
                .collect()
        })
}

/// Builds the worker context exclusively from DB-02 question and prediction
/// interfaces. Missing or incomplete question contracts are rejected rather
/// than assigned a guessed horizon, scale, or provider.
pub async fn load_advanced_attempt_context(
    pool: &PgPool,
    attempt_id: Uuid,
    user_id: Uuid,
) -> Result<AdvancedAttemptContext, AdvancedSettlementError> {
    let Some(attempt) = quiz_attempts::find_by_id(pool, attempt_id).await? else {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    };
    if attempt.user_id != user_id
        || attempt.quiz_type != "advanced"
        || attempt.status != "pending" && attempt.status != ATTEMPT_COMPLETED
    {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }

    let predictions = load_advanced_predictions(pool, attempt_id).await?;
    if predictions.len() != usize::try_from(attempt.total_questions).unwrap_or(0) {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }
    let question_ids = predictions
        .iter()
        .map(|prediction| prediction.question_id)
        .collect::<Vec<_>>();
    let contracts = quiz_questions::advanced_contracts_by_question_ids(pool, &question_ids).await?;
    if contracts.len() != predictions.len() {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }

    let mut contracts = contracts
        .into_iter()
        .map(|contract| (contract.id, contract))
        .collect::<HashMap<_, _>>();
    let mut resolutions = Vec::with_capacity(predictions.len());
    for prediction in predictions {
        let Some(contract) = contracts.remove(&prediction.question_id) else {
            return Err(AdvancedSettlementError::InvalidAttemptContext);
        };
        let value_scale = u32::try_from(contract.value_scale)
            .map_err(|_| AdvancedSettlementError::InvalidAttemptContext)?;
        resolutions.push(AdvancedResolution {
            question: AdvancedQuestion {
                id: contract.id,
                value_scale,
                horizon_at: contract.horizon_at,
                expires_at: contract.expires_at,
                provider_key: contract.provider_key,
            },
            prediction,
        });
    }

    Ok(AdvancedAttemptContext {
        attempt_id,
        user_id,
        resolutions,
    })
}

/// Typed consumer entry point for the durable numeric-submission event.
/// Payload values are only routing metadata; predictions and question facts
/// are reloaded from PostgreSQL before the provider and settlement are called.
pub async fn process_advanced_submission_event(
    pool: &PgPool,
    provider: &impl AdvancedActualProvider,
    event: &orion_db::models::OutboxEvent,
) -> Result<AdvancedSettlementOutcome, AdvancedSettlementError> {
    let schema_version = u16::try_from(event.schema_version)
        .map_err(|_| AdvancedSettlementError::InvalidAttemptContext)?;
    if event.event_type != ADVANCED_SUBMITTED_EVENT_TYPE
        || ensure_event_compatible(event.event_type.as_str(), schema_version).is_err()
    {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }
    let request: AdvancedSubmissionRequestedV1 = serde_json::from_value(event.payload.clone())
        .map_err(|_| AdvancedSettlementError::InvalidAttemptContext)?;
    let context = load_advanced_attempt_context(pool, request.attempt_id, request.user_id).await?;
    let loaded_question_ids = context
        .resolutions
        .iter()
        .map(|resolution| resolution.question.id)
        .collect::<std::collections::HashSet<_>>();
    let requested_question_ids = request
        .question_ids
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    if loaded_question_ids != requested_question_ids {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }
    settle_pending_advanced_with_retry(pool, provider, &context, RetryPolicy::default()).await
}

/// Settles one pending attempt once. The provider is called once per
/// resolution. Validation happens before the DB transaction, and the DB-02
/// transaction then performs the server-side score, Elo update, immutable
/// rating events, and pending-to-completed transition atomically.
pub async fn settle_pending_advanced_attempt<P>(
    pool: &PgPool,
    provider: &P,
    context: &AdvancedAttemptContext,
) -> Result<AdvancedSettlementOutcome, AdvancedSettlementError>
where
    P: AdvancedActualProvider,
{
    settle_pending_advanced_attempt_with_hooks(pool, provider, context, &NoopSettlementHooks).await
}

/// Testable form of [`settle_pending_advanced_attempt`]. Production callers
/// use the no-op hook; recovery tests inject a single crash at each boundary
/// and then redeliver the same idempotency key.
pub async fn settle_pending_advanced_attempt_with_hooks<P, H>(
    pool: &PgPool,
    provider: &P,
    context: &AdvancedAttemptContext,
    hooks: &H,
) -> Result<AdvancedSettlementOutcome, AdvancedSettlementError>
where
    P: AdvancedActualProvider,
    H: AdvancedSettlementHooks,
{
    let Some(pending) =
        quiz_attempts::find_pending_advanced_by_id(pool, context.attempt_id, context.user_id)
            .await?
    else {
        let Some(attempt) = quiz_attempts::find_by_id(pool, context.attempt_id).await? else {
            return Err(AdvancedSettlementError::InvalidAttemptContext);
        };
        if attempt.user_id != context.user_id || attempt.quiz_type != "advanced" {
            return Err(AdvancedSettlementError::InvalidAttemptContext);
        }
        if attempt.status != ATTEMPT_COMPLETED {
            return Err(AdvancedSettlementError::InvalidAttemptContext);
        }
        let result = completed_result(pool, attempt).await?;
        ensure_outbox_events(pool, &result)
            .await
            .map_err(AdvancedSettlementError::Outbox)?;
        return Ok(AdvancedSettlementOutcome::AlreadyCompleted(result));
    };

    hooks.reached(AdvancedSettlementBoundary::AfterPendingLookup)?;

    if pending.id != context.attempt_id
        || pending.user_id != context.user_id
        || pending.total_questions as usize != context.resolutions.len()
        || context.resolutions.is_empty()
    {
        return Err(AdvancedSettlementError::InvalidAttemptContext);
    }

    let resolutions = resolve_and_validate(provider, &context.resolutions).await?;
    hooks.reached(AdvancedSettlementBoundary::AfterActualValidation)?;
    hooks.reached(AdvancedSettlementBoundary::BeforeAtomicSettlement)?;
    let input = AdvancedSettlementInput {
        attempt_id: pending.id,
        user_id: pending.user_id,
        resolutions,
        started_at: pending.started_at,
        completed_at: Utc::now(),
    };

    // This is deliberately the only call to the atomic Advanced settlement
    // interface. It owns domain scoring, Elo, rating_events, and the
    // pending-to-completed transition.
    let result = settle_advanced_actual_quiz(pool, input).await?;
    hooks.reached(AdvancedSettlementBoundary::AfterAtomicSettlement)?;
    hooks.reached(AdvancedSettlementBoundary::BeforeOutbox)?;
    ensure_outbox_events(pool, &result)
        .await
        .map_err(AdvancedSettlementError::Outbox)?;
    hooks.reached(AdvancedSettlementBoundary::AfterOutbox)?;
    Ok(AdvancedSettlementOutcome::Completed(result))
}

/// Runs the worker retry policy. Transient provider/database/outbox failures
/// are retried with bounded backoff; validation and terminal provider errors
/// are recorded as a durable dead-letter event immediately.
pub async fn settle_pending_advanced_with_retry<P>(
    pool: &PgPool,
    provider: &P,
    context: &AdvancedAttemptContext,
    policy: RetryPolicy,
) -> Result<AdvancedSettlementOutcome, AdvancedSettlementError>
where
    P: AdvancedActualProvider,
{
    settle_pending_advanced_with_retry_and_hooks(
        pool,
        provider,
        context,
        policy,
        &NoopSettlementHooks,
    )
    .await
}

pub async fn settle_pending_advanced_with_retry_and_hooks<P, H>(
    pool: &PgPool,
    provider: &P,
    context: &AdvancedAttemptContext,
    policy: RetryPolicy,
    hooks: &H,
) -> Result<AdvancedSettlementOutcome, AdvancedSettlementError>
where
    P: AdvancedActualProvider,
    H: AdvancedSettlementHooks,
{
    let max_attempts = policy
        .max_attempts
        .clamp(1, MAX_ADVANCED_SETTLEMENT_ATTEMPTS);
    for attempt in 1..=max_attempts {
        match settle_pending_advanced_attempt_with_hooks(pool, provider, context, hooks).await {
            Ok(outcome) => return Ok(outcome),
            Err(error) if error.is_retryable() && attempt < max_attempts => {
                if matches!(error, AdvancedSettlementError::ProviderUnavailable) {
                    ADVANCED_PROVIDER_OUTAGES
                        .get_or_init(|| AtomicU64::new(0))
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        target: "orion.alerts",
                        alert = ADVANCED_PROVIDER_OUTAGE_ALERT,
                        attempt_id = %context.attempt_id,
                        attempt,
                        "Advanced actual provider is unavailable; the attempt remains pending"
                    );
                }
                let delay = policy
                    .delay(attempt, context.attempt_id.as_u128() as u64)
                    .unwrap_or_default();
                tracing::warn!(
                    target: "orion.worker",
                    attempt_id = %context.attempt_id,
                    attempt,
                    outcome = "retry_scheduled",
                    "Advanced settlement will retry after a transient failure"
                );
                sleep(delay).await;
            }
            Err(error) => {
                if matches!(error, AdvancedSettlementError::ProviderUnavailable) {
                    ADVANCED_PROVIDER_OUTAGES
                        .get_or_init(|| AtomicU64::new(0))
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::error!(
                        target: "orion.alerts",
                        alert = ADVANCED_PROVIDER_OUTAGE_ALERT,
                        attempt_id = %context.attempt_id,
                        attempt,
                        "Advanced actual provider outage exhausted the bounded retry budget; the attempt remains pending"
                    );
                }
                ensure_dead_letter(pool, context, &error)
                    .await
                    .map_err(AdvancedSettlementError::Outbox)?;
                tracing::error!(
                    target: "orion.worker",
                    attempt_id = %context.attempt_id,
                    outcome = "dead_lettered",
                    "Advanced settlement reached its terminal retry outcome"
                );
                return Ok(AdvancedSettlementOutcome::DeadLettered);
            }
        }
    }

    Err(AdvancedSettlementError::InvalidAttemptContext)
}

async fn resolve_and_validate<P>(
    provider: &P,
    resolutions: &[AdvancedResolution],
) -> Result<Vec<AdvancedSettlementResolution>, AdvancedSettlementError>
where
    P: AdvancedActualProvider,
{
    let mut resolved = Vec::with_capacity(resolutions.len());
    let mut question_ids = HashMap::with_capacity(resolutions.len());
    for resolution in resolutions {
        validate_advanced_prediction(&resolution.question, &resolution.prediction)?;
        if question_ids.insert(resolution.question.id, ()).is_some() {
            return Err(AdvancedSettlementError::InvalidAttemptContext);
        }

        let actual = match provider.obtain_actual(&resolution.question).await {
            Ok(actual) => actual,
            Err(ActualProviderError::Unavailable) => {
                return Err(AdvancedSettlementError::ProviderUnavailable)
            }
            Err(ActualProviderError::Terminal) => {
                return Err(AdvancedSettlementError::ProviderTerminal)
            }
        };
        validate_advanced_actual_value(&resolution.question, &actual.value)?;
        if actual.value.available_at > Utc::now() {
            return Err(AdvancedSettlementError::ActualNotAvailable);
        }
        resolved.push(AdvancedSettlementResolution {
            prediction: resolution.prediction.clone(),
            actual: actual.value,
        });
    }
    Ok(resolved)
}

fn validate_advanced_prediction(
    question: &AdvancedQuestion,
    prediction: &AdvancedPrediction,
) -> Result<(), AdvancedValidationError> {
    validate_question_window(question)?;
    if prediction.question_id != question.id {
        return Err(AdvancedValidationError::PredictionQuestionMismatch);
    }
    if prediction.value.scale() > question.value_scale {
        return Err(AdvancedValidationError::ValueScaleExceeded);
    }
    if prediction.submitted_at >= question.horizon_at {
        return Err(AdvancedValidationError::PredictionAfterHorizon);
    }
    Ok(())
}

fn validate_advanced_actual_value(
    question: &AdvancedQuestion,
    actual: &AdvancedActualValue,
) -> Result<(), AdvancedValidationError> {
    validate_question_window(question)?;
    if actual.question_id != question.id {
        return Err(AdvancedValidationError::PredictionQuestionMismatch);
    }
    if actual.value.scale() > question.value_scale {
        return Err(AdvancedValidationError::ValueScaleExceeded);
    }
    if !actual.is_final {
        return Err(AdvancedValidationError::ActualNotFinal);
    }
    if actual.observed_at > question.horizon_at {
        return Err(AdvancedValidationError::ActualObservedAfterHorizon);
    }
    if actual.available_at < question.horizon_at {
        return Err(AdvancedValidationError::ActualAvailableBeforeHorizon);
    }
    if actual.available_at > question.expires_at {
        return Err(AdvancedValidationError::ActualAvailableAfterExpiry);
    }
    if actual.available_at < actual.observed_at {
        return Err(AdvancedValidationError::ActualAvailableBeforeObserved);
    }
    if actual.source_id.trim().is_empty() || actual.source_version.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyActualSource);
    }
    if actual.source_id.trim() != question.provider_key.trim() {
        return Err(AdvancedValidationError::ActualSourceMismatch);
    }
    Ok(())
}

fn validate_question_window(question: &AdvancedQuestion) -> Result<(), AdvancedValidationError> {
    if question.expires_at <= question.horizon_at {
        return Err(AdvancedValidationError::InvalidQuestionWindow);
    }
    Ok(())
}

async fn completed_result(
    pool: &PgPool,
    attempt: QuizAttempt,
) -> Result<QuizSettlementResult, sqlx::Error> {
    let Some(user_rating) = ratings::get_user_rating(pool, attempt.user_id).await? else {
        return Err(sqlx::Error::RowNotFound);
    };
    let events = ratings::rating_events_by_attempt_id(pool, attempt.id).await?;
    Ok(QuizSettlementResult {
        attempt,
        user_rating,
        events,
    })
}

async fn ensure_outbox_events(
    pool: &PgPool,
    result: &QuizSettlementResult,
) -> Result<(), sqlx::Error> {
    let attempt_id = result.attempt.id;
    let dedupe_key = format!("advanced-settlement:{attempt_id}");
    let question_ids = result
        .events
        .iter()
        .map(|event| event.question_id)
        .collect::<Vec<_>>();
    let rating_events = result
        .events
        .iter()
        .map(|event| {
            json!({
                "event_id": event.id,
                "question_id": event.question_id,
                "correct": event.correct,
                "rating_delta": event.rating_delta,
            })
        })
        .collect::<Vec<_>>();

    let settlement_payload = json!({
        "schema_version": ADVANCED_SETTLEMENT_SCHEMA_VERSION,
        "dedupe_key": dedupe_key,
        "attempt_id": attempt_id,
        "user_id": result.attempt.user_id,
        "status": result.attempt.status,
        "rating_after": result.attempt.rating_after,
        "events": rating_events,
    });
    let cache_payload = json!({
        "schema_version": ADVANCED_SETTLEMENT_SCHEMA_VERSION,
        "dedupe_key": format!("{dedupe_key}:cache"),
        "attempt_id": attempt_id,
        "user_id": result.attempt.user_id,
        "question_ids": question_ids,
    });
    let notification_payload = json!({
        "schema_version": ADVANCED_SETTLEMENT_SCHEMA_VERSION,
        "dedupe_key": format!("{dedupe_key}:notification"),
        "deduplication_key": format!("{dedupe_key}:notification"),
        "notification_id": attempt_id,
        "recipient_id": result.attempt.user_id,
        "kind": "rating_changed",
        "title": "Advanced quiz settled",
        "body": "Your Advanced quiz result and rating have been updated.",
        "action_url": format!("/quiz/attempts/{attempt_id}"),
    });

    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&dedupe_key)
        .execute(&mut *transaction)
        .await?;
    insert_outbox_if_absent(
        &mut transaction,
        ADVANCED_SETTLED_EVENT_TYPE,
        &settlement_payload,
    )
    .await?;
    insert_outbox_if_absent(
        &mut transaction,
        ADVANCED_CACHE_INVALIDATION_EVENT_TYPE,
        &cache_payload,
    )
    .await?;
    insert_outbox_if_absent(
        &mut transaction,
        ADVANCED_NOTIFICATION_EVENT_TYPE,
        &notification_payload,
    )
    .await?;
    transaction.commit().await
}

async fn insert_outbox_if_absent(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_type: &str,
    payload: &Value,
) -> Result<(), sqlx::Error> {
    let dedupe_key = payload
        .get("dedupe_key")
        .and_then(Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("outbox payload has no dedupe key".to_owned()))?;
    sqlx::query(
        "INSERT INTO outbox_events (event_type, schema_version, payload)
         SELECT $1, $2, $3
         WHERE NOT EXISTS (
             SELECT 1 FROM outbox_events
             WHERE event_type = $1 AND payload ->> 'dedupe_key' = $4
         )",
    )
    .bind(event_type)
    .bind(ADVANCED_SETTLEMENT_SCHEMA_VERSION)
    .bind(sqlx::types::Json(payload))
    .bind(dedupe_key)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn ensure_dead_letter(
    pool: &PgPool,
    context: &AdvancedAttemptContext,
    error: &AdvancedSettlementError,
) -> Result<(), sqlx::Error> {
    let payload = json!({
        "schema_version": ADVANCED_SETTLEMENT_SCHEMA_VERSION,
        "dedupe_key": format!("advanced-settlement:{0}:dead-letter", context.attempt_id),
        "attempt_id": context.attempt_id,
        "user_id": context.user_id,
        "reason": dead_letter_reason(error),
    });
    let mut transaction = pool.begin().await?;
    let lock_key = format!("advanced-settlement:{}:dead-letter", context.attempt_id);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *transaction)
        .await?;
    insert_outbox_if_absent(&mut transaction, ADVANCED_DEAD_LETTER_EVENT_TYPE, &payload).await?;
    transaction.commit().await
}

fn dead_letter_reason(error: &AdvancedSettlementError) -> &'static str {
    match error {
        AdvancedSettlementError::ProviderTerminal => "provider_terminal_failure",
        AdvancedSettlementError::InvalidActual(_) => "invalid_actual_value",
        AdvancedSettlementError::InvalidAttemptContext => "invalid_attempt_context",
        AdvancedSettlementError::ActualNotAvailable
        | AdvancedSettlementError::ProviderUnavailable
        | AdvancedSettlementError::Database(_)
        | AdvancedSettlementError::Outbox(_)
        | AdvancedSettlementError::InjectedCrash(_) => "retry_budget_exhausted",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };

    use chrono::{Duration as ChronoDuration, Utc};
    use rust_decimal::Decimal;

    use super::*;

    struct CrashOnce {
        boundary: AdvancedSettlementBoundary,
        fired: AtomicBool,
    }

    impl CrashOnce {
        fn new(boundary: AdvancedSettlementBoundary) -> Self {
            Self {
                boundary,
                fired: AtomicBool::new(false),
            }
        }
    }

    impl AdvancedSettlementHooks for CrashOnce {
        fn reached(
            &self,
            boundary: AdvancedSettlementBoundary,
        ) -> Result<(), AdvancedSettlementError> {
            if boundary == self.boundary && !self.fired.swap(true, Ordering::SeqCst) {
                return Err(AdvancedSettlementError::InjectedCrash(boundary));
            }
            Ok(())
        }
    }
    struct CountingProvider {
        calls: Arc<AtomicUsize>,
        result: Result<ResolvedActual, ActualProviderError>,
    }

    impl AdvancedActualProvider for CountingProvider {
        fn obtain_actual<'a>(&'a self, _question: &'a AdvancedQuestion) -> ActualFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn resolution() -> AdvancedResolution {
        let now = Utc::now();
        let question_id = Uuid::from_u128(1);
        let question = AdvancedQuestion {
            id: question_id,
            value_scale: 2,
            horizon_at: now - ChronoDuration::minutes(2),
            expires_at: now + ChronoDuration::minutes(2),
            provider_key: "provider".to_owned(),
        };
        AdvancedResolution {
            prediction: AdvancedPrediction {
                question_id,
                value: Decimal::new(100, 2),
                submitted_at: now - ChronoDuration::minutes(3),
            },
            question,
        }
    }

    fn actual(resolution: &AdvancedResolution) -> ResolvedActual {
        ResolvedActual {
            value: AdvancedActualValue {
                question_id: resolution.question.id,
                value: Decimal::new(100, 2),
                observed_at: resolution.question.horizon_at,
                available_at: resolution.question.horizon_at + ChronoDuration::minutes(1),
                source_id: "provider".to_owned(),
                source_version: "1".to_owned(),
                is_final: true,
            },
        }
    }

    #[tokio::test]
    async fn provider_is_called_once_and_resolution_is_validated() {
        let resolution = resolution();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = CountingProvider {
            calls: Arc::clone(&calls),
            result: Ok(actual(&resolution)),
        };

        let resolutions = resolve_and_validate(&provider, std::slice::from_ref(&resolution))
            .await
            .expect("valid actual");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].prediction, resolution.prediction);
        assert_eq!(resolutions[0].actual.value, Decimal::new(100, 2));
    }

    #[tokio::test]
    async fn provider_outage_is_retryable_and_does_not_produce_answers() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = CountingProvider {
            calls: Arc::clone(&calls),
            result: Err(ActualProviderError::Unavailable),
        };

        let result = resolve_and_validate(&provider, &[resolution()]).await;

        assert!(matches!(
            result,
            Err(AdvancedSettlementError::ProviderUnavailable)
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn terminal_errors_are_dead_lettered_but_outages_are_retryable() {
        assert!(!AdvancedSettlementError::ProviderTerminal.is_retryable());
        assert!(AdvancedSettlementError::ProviderUnavailable.is_retryable());
        assert_eq!(
            dead_letter_reason(&AdvancedSettlementError::ProviderTerminal),
            "provider_terminal_failure"
        );
    }

    #[test]
    fn every_recovery_boundary_is_injectable_once() {
        let boundaries = [
            AdvancedSettlementBoundary::AfterPendingLookup,
            AdvancedSettlementBoundary::AfterActualValidation,
            AdvancedSettlementBoundary::BeforeAtomicSettlement,
            AdvancedSettlementBoundary::AfterAtomicSettlement,
            AdvancedSettlementBoundary::BeforeOutbox,
            AdvancedSettlementBoundary::AfterOutbox,
        ];

        for boundary in boundaries {
            let hooks = CrashOnce::new(boundary);
            assert!(matches!(
                hooks.reached(boundary),
                Err(AdvancedSettlementError::InjectedCrash(actual)) if actual == boundary
            ));
            assert!(hooks.reached(boundary).is_ok());
        }
    }
}
