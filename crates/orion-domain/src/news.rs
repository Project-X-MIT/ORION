use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Version of the normalized market-news contract.
pub const NEWS_CONTRACT_VERSION: u16 = 1;

/// A configured upstream feed whose rows have passed source admission.
///
/// The source's license is checked by ingestion configuration before a source
/// can be admitted. It is intentionally not inferred from the presence of a
/// row in `news_sources`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsSource {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub external_id: Option<String>,
    pub source_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NewsSource {
    /// Canonicalizes text, identifiers and URLs at the domain boundary.
    pub fn normalize(&mut self) -> Result<(), NewsValidationError> {
        self.name = normalize_text("name", &self.name, 200)?;
        self.slug = normalize_slug(&self.slug)?;
        self.external_id = normalize_identifier("external_id", self.external_id.as_deref())?;
        self.source_url = normalize_optional_url("source_url", self.source_url.as_deref())?;
        Ok(())
    }

    /// Verifies that this value has already been normalized and is safe to
    /// pass to a feed or persistence boundary.
    pub fn validate(&self) -> Result<(), NewsValidationError> {
        let mut normalized = self.clone();
        normalized.normalize()?;
        if normalized != *self {
            return Err(NewsValidationError::NotNormalized { field: "source" });
        }
        validate_timestamp_order("created_at", self.created_at, "updated_at", self.updated_at)?;
        Ok(())
    }
}

/// The minimum attribution that the public feed renders for every article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributionPolicy {
    SourceAndArticleLink,
}

/// Auditable evidence that a source may be fetched, stored and redistributed.
///
/// This is provider configuration, not a replacement for a database column in
/// `news_sources`. The presence of an approval is the explicit admission
/// decision; an unknown source has no implicit license.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLicenseApproval {
    pub source_id: Uuid,
    pub license_name: String,
    pub license_url: String,
    pub feed_redistribution_allowed: bool,
    pub attribution: AttributionPolicy,
    pub approved_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl SourceLicenseApproval {
    /// Canonicalizes the human-readable license name and evidence URL.
    pub fn normalize(&mut self) -> Result<(), NewsValidationError> {
        self.license_name = normalize_text("license_name", &self.license_name, 200)?;
        self.license_url = sanitize_url_field("license_url", &self.license_url)?;
        Ok(())
    }

    /// Verifies this approval is normalized and active at `at`.
    pub fn validate_at(&self, at: DateTime<Utc>) -> Result<(), NewsValidationError> {
        let mut normalized = self.clone();
        normalized.normalize()?;
        if normalized != *self {
            return Err(NewsValidationError::NotNormalized {
                field: "source_license_approval",
            });
        }
        if !self.feed_redistribution_allowed {
            return Err(NewsValidationError::FeedRedistributionNotAllowed);
        }
        if at < self.approved_at {
            return Err(NewsValidationError::LicenseNotActive);
        }
        if self.expires_at.is_some_and(|expires_at| at >= expires_at) {
            return Err(NewsValidationError::LicenseExpired);
        }
        Ok(())
    }

    /// Checks that this approval belongs to `source_id` and is currently
    /// usable for public feed publication.
    pub fn validate_for(
        &self,
        source_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), NewsValidationError> {
        if self.source_id != source_id {
            return Err(NewsValidationError::SourceApprovalMismatch);
        }
        self.validate_at(at)
    }
}

/// Safe public attribution generated from a normalized source and article.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsAttribution {
    pub source_name: String,
    pub article_url: String,
}

/// Ordered deduplication candidates matching the database uniqueness rules.
///
/// The canonical URL is always primary because `news_articles.url` is
/// globally unique. A provider external ID is a source-scoped fallback when
/// it is available.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NewsDeduplicationKey {
    CanonicalUrl(String),
    SourceExternalId {
        source_id: Uuid,
        external_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsDeduplicationIdentity {
    pub primary: NewsDeduplicationKey,
    pub fallback: Option<NewsDeduplicationKey>,
}

/// A normalized market-news article stored in `news_articles`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsArticle {
    pub id: Uuid,
    pub source_id: Uuid,
    pub external_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub url: String,
    pub image_url: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub symbols: Vec<String>,
    pub published_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NewsArticle {
    /// Canonicalizes article text, URLs, categories and market symbols.
    pub fn normalize(&mut self) -> Result<(), NewsValidationError> {
        self.external_id = normalize_identifier("external_id", self.external_id.as_deref())?;
        self.title = normalize_text("title", &self.title, 500)?;
        self.summary = normalize_text("summary", &self.summary, 4_000)?;
        self.content = sanitize_content(&self.content)?;
        self.url = sanitize_url_field("url", &self.url)?;
        self.image_url = normalize_optional_url("image_url", self.image_url.as_deref())?;
        self.author = normalize_optional_text("author", self.author.as_deref(), 300)?;
        self.category = normalize_optional_text("category", self.category.as_deref(), 100)?
            .map(|category| category.to_ascii_lowercase());

        let mut symbols = self
            .symbols
            .iter()
            .map(|symbol| normalize_symbol(symbol))
            .collect::<Result<Vec<_>, _>>()?;
        symbols.sort_unstable();
        symbols.dedup();
        self.symbols = symbols;
        Ok(())
    }

    /// Verifies that this value has already been normalized and is safe to
    /// publish or persist.
    pub fn validate(&self) -> Result<(), NewsValidationError> {
        let mut normalized = self.clone();
        normalized.normalize()?;
        if normalized != *self {
            return Err(NewsValidationError::NotNormalized { field: "article" });
        }
        validate_timestamp_order("created_at", self.created_at, "updated_at", self.updated_at)?;
        validate_timestamp_order(
            "ingested_at",
            self.ingested_at,
            "updated_at",
            self.updated_at,
        )?;
        Ok(())
    }

    /// Returns the ordered identity candidates used for idempotent ingestion.
    pub fn deduplication_identity(&self) -> Result<NewsDeduplicationIdentity, NewsValidationError> {
        self.validate()?;
        Ok(NewsDeduplicationIdentity {
            primary: NewsDeduplicationKey::CanonicalUrl(self.url.clone()),
            fallback: self.external_id.as_ref().map(|external_id| {
                NewsDeduplicationKey::SourceExternalId {
                    source_id: self.source_id,
                    external_id: external_id.clone(),
                }
            }),
        })
    }

    /// Validates that an article is eligible for a feed at the supplied UTC
    /// instant. Future-dated provider content remains persisted but hidden.
    pub fn validate_for_feed_at(&self, at: DateTime<Utc>) -> Result<(), NewsValidationError> {
        self.validate()?;
        if self.published_at > at {
            return Err(NewsValidationError::FuturePublicationTimestamp);
        }
        Ok(())
    }

    /// Classifies article age using UTC publication and observation times.
    pub fn freshness_at(
        &self,
        observed_at: DateTime<Utc>,
        policy: &NewsFreshnessPolicy,
    ) -> Result<NewsFreshnessSignal, FreshnessPolicyError> {
        self.validate()
            .map_err(FreshnessPolicyError::InvalidArticle)?;
        policy.validate()?;
        Ok(policy.classify(self.published_at, observed_at))
    }

    /// Creates the only public attribution form permitted by the contract:
    /// the admitted source name and the article's canonical original URL.
    pub fn attribution(
        &self,
        source: &NewsSource,
        approval: &SourceLicenseApproval,
        at: DateTime<Utc>,
    ) -> Result<NewsAttribution, NewsValidationError> {
        self.validate()?;
        source.validate()?;
        if self.source_id != source.id {
            return Err(NewsValidationError::ArticleSourceMismatch);
        }
        approval.validate_for(source.id, at)?;
        match approval.attribution {
            AttributionPolicy::SourceAndArticleLink => Ok(NewsAttribution {
                source_name: source.name.clone(),
                article_url: self.url.clone(),
            }),
        }
    }
}

/// Age class of an article at a UTC observation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewsFreshness {
    FutureDated,
    Fresh,
    Aging,
    Stale,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsFreshnessPolicy {
    pub fresh_for_seconds: i64,
    pub aging_for_seconds: i64,
    pub stale_after_seconds: i64,
}

impl Default for NewsFreshnessPolicy {
    fn default() -> Self {
        Self {
            fresh_for_seconds: 900,
            aging_for_seconds: 3_600,
            stale_after_seconds: 86_400,
        }
    }
}

impl NewsFreshnessPolicy {
    pub fn validate(&self) -> Result<(), FreshnessPolicyError> {
        if self.fresh_for_seconds <= 0
            || self.aging_for_seconds <= self.fresh_for_seconds
            || self.stale_after_seconds <= self.aging_for_seconds
        {
            return Err(FreshnessPolicyError::InvalidThresholdOrder);
        }
        Ok(())
    }

    #[must_use]
    pub fn classify(
        &self,
        published_at: DateTime<Utc>,
        observed_at: DateTime<Utc>,
    ) -> NewsFreshnessSignal {
        let is_future = published_at > observed_at;
        let age_seconds = if is_future {
            -observed_at
                .signed_duration_since(published_at)
                .num_seconds()
                .max(1)
        } else {
            observed_at
                .signed_duration_since(published_at)
                .num_seconds()
        };
        let status = if is_future {
            NewsFreshness::FutureDated
        } else if age_seconds <= self.fresh_for_seconds {
            NewsFreshness::Fresh
        } else if age_seconds <= self.aging_for_seconds {
            NewsFreshness::Aging
        } else if age_seconds <= self.stale_after_seconds {
            NewsFreshness::Stale
        } else {
            NewsFreshness::Expired
        };

        NewsFreshnessSignal {
            status,
            published_at,
            observed_at,
            age_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsFreshnessSignal {
    pub status: NewsFreshness,
    pub published_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
    /// Negative while the provider publication time is in the future.
    pub age_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FreshnessPolicyError {
    #[error("freshness thresholds must be positive and strictly ordered")]
    InvalidThresholdOrder,
    #[error("article is invalid: {0}")]
    InvalidArticle(NewsValidationError),
}

/// Safe, structured ingestion signal. It deliberately carries no raw
/// provider response or error text because those may contain secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsErrorSignalKind {
    Provider(ProviderFailureKind),
    NormalizationRejected,
    ValidationRejected,
    DeduplicationConflict,
    PersistenceFailed,
    CircuitOpen,
}

impl NewsErrorSignalKind {
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Provider(failure) if failure.is_retryable())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsErrorSignal {
    pub source_id: Uuid,
    pub article_id: Option<Uuid>,
    pub kind: NewsErrorSignalKind,
    pub occurred_at: DateTime<Utc>,
    /// One-based attempt number, including the initial request.
    pub attempt: u8,
}

impl NewsErrorSignal {
    pub fn validate(&self) -> Result<(), NewsValidationError> {
        if self.attempt == 0 {
            return Err(NewsValidationError::InvalidAttempt);
        }
        Ok(())
    }

    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }

    #[must_use]
    pub fn retry_delay_ms(&self, policy: &ProviderRuntimePolicy) -> Option<u64> {
        match self.kind {
            NewsErrorSignalKind::Provider(failure) => policy.retry_delay_ms(self.attempt, failure),
            _ => None,
        }
    }
}

/// A durable attempt to ingest one source from `news_ingestion_runs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsIngestionRun {
    pub id: Uuid,
    pub source_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: IngestionRunStatus,
    pub articles_seen: i32,
    pub articles_inserted: i32,
    pub error_message: Option<String>,
}

impl NewsIngestionRun {
    pub fn normalize(&mut self) -> Result<(), NewsValidationError> {
        self.error_message =
            normalize_optional_text("error_message", self.error_message.as_deref(), 2_000)?;
        Ok(())
    }

    #[must_use]
    pub const fn is_completed(&self) -> bool {
        matches!(self.status, IngestionRunStatus::Completed) && self.completed_at.is_some()
    }

    pub fn validate(&self) -> Result<(), NewsValidationError> {
        let mut normalized = self.clone();
        normalized.normalize()?;
        if normalized != *self {
            return Err(NewsValidationError::NotNormalized {
                field: "ingestion_run",
            });
        }
        if self.articles_seen < 0 || self.articles_inserted < 0 {
            return Err(NewsValidationError::NegativeIngestionCount);
        }
        if self.articles_inserted > self.articles_seen {
            return Err(NewsValidationError::InsertedExceedsSeen);
        }
        if let Some(completed_at) = self.completed_at {
            validate_timestamp_order("started_at", self.started_at, "completed_at", completed_at)?;
        }
        match (self.status, self.completed_at.is_some()) {
            (IngestionRunStatus::Running, true)
            | (IngestionRunStatus::Completed | IngestionRunStatus::Failed, false) => {
                Err(NewsValidationError::IngestionCompletionMismatch)
            }
            _ => Ok(()),
        }
    }
}

/// Article progression from an untrusted provider response to the public feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArticleLifecycle {
    Fetched,
    Normalized,
    Validated,
    Deduplicated,
    Persisted,
    Published,
}

impl ArticleLifecycle {
    /// Returns whether `next` is the next permitted lifecycle stage.
    #[must_use]
    pub const fn can_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Fetched, Self::Normalized)
                | (Self::Normalized, Self::Validated)
                | (Self::Validated, Self::Deduplicated)
                | (Self::Deduplicated, Self::Persisted)
                | (Self::Persisted, Self::Published)
        )
    }
}

/// Operational health state for an admitted news provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealth {
    Healthy,
    Degraded,
    CircuitOpen,
    Probing,
}

impl ProviderHealth {
    /// Returns whether a provider may move directly between these states.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Healthy, Self::Degraded | Self::CircuitOpen)
                | (Self::Degraded, Self::Healthy | Self::CircuitOpen)
                | (Self::CircuitOpen, Self::Probing)
                | (
                    Self::Probing,
                    Self::Healthy | Self::Degraded | Self::CircuitOpen
                )
        )
    }
}

/// Failure classes used to decide whether a provider request may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFailureKind {
    Timeout,
    Transport,
    RateLimited,
    UpstreamServer,
    InvalidPayload,
    Unauthorized,
    LicenseViolation,
}

impl ProviderFailureKind {
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Transport | Self::RateLimited | Self::UpstreamServer
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTimeoutPolicy {
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRateLimitPolicy {
    pub max_requests_per_window: u32,
    pub window_seconds: u32,
    pub burst_capacity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRetryPolicy {
    /// Total attempts, including the initial request.
    pub max_attempts: u8,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCircuitBreakerPolicy {
    pub failure_threshold: u32,
    pub open_for_seconds: u32,
    pub probe_limit: u32,
    pub successes_to_close: u32,
}

/// Framework-neutral provider controls consumed by the worker and adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuntimePolicy {
    pub timeout: ProviderTimeoutPolicy,
    pub rate_limit: ProviderRateLimitPolicy,
    pub retry: ProviderRetryPolicy,
    pub circuit_breaker: ProviderCircuitBreakerPolicy,
}

impl Default for ProviderRuntimePolicy {
    fn default() -> Self {
        Self {
            timeout: ProviderTimeoutPolicy {
                request_timeout_ms: 10_000,
            },
            rate_limit: ProviderRateLimitPolicy {
                max_requests_per_window: 60,
                window_seconds: 60,
                burst_capacity: 5,
            },
            retry: ProviderRetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 250,
                max_backoff_ms: 4_000,
            },
            circuit_breaker: ProviderCircuitBreakerPolicy {
                failure_threshold: 5,
                open_for_seconds: 60,
                probe_limit: 1,
                successes_to_close: 2,
            },
        }
    }
}

impl ProviderRuntimePolicy {
    pub fn validate(&self) -> Result<(), ProviderPolicyError> {
        if self.timeout.request_timeout_ms == 0 || self.timeout.request_timeout_ms > 10_000 {
            return Err(ProviderPolicyError::OutOfRange {
                field: "timeout.request_timeout_ms",
            });
        }
        if self.rate_limit.max_requests_per_window == 0 || self.rate_limit.window_seconds == 0 {
            return Err(ProviderPolicyError::NotPositive {
                field: "rate_limit",
            });
        }
        if self.rate_limit.burst_capacity == 0
            || self.rate_limit.burst_capacity > self.rate_limit.max_requests_per_window
            || self.rate_limit.burst_capacity > 5
            || u64::from(self.rate_limit.max_requests_per_window)
                > u64::from(self.rate_limit.window_seconds)
        {
            return Err(ProviderPolicyError::Inconsistent {
                field: "rate_limit.burst_capacity",
                related: "rate_limit.max_requests_per_window",
            });
        }
        if self.retry.max_attempts == 0 || self.retry.max_attempts > 3 {
            return Err(ProviderPolicyError::OutOfRange {
                field: "retry.max_attempts",
            });
        }
        if self.retry.initial_backoff_ms == 0
            || self.retry.max_backoff_ms < self.retry.initial_backoff_ms
            || self.retry.max_backoff_ms > 4_000
        {
            return Err(ProviderPolicyError::Inconsistent {
                field: "retry.max_backoff_ms",
                related: "retry.initial_backoff_ms",
            });
        }
        if self.circuit_breaker.failure_threshold == 0 {
            return Err(ProviderPolicyError::NotPositive {
                field: "circuit_breaker.failure_threshold",
            });
        }
        if self.circuit_breaker.open_for_seconds < 60 {
            return Err(ProviderPolicyError::OutOfRange {
                field: "circuit_breaker.open_for_seconds",
            });
        }
        if self.circuit_breaker.probe_limit != 1 {
            return Err(ProviderPolicyError::OutOfRange {
                field: "circuit_breaker.probe_limit",
            });
        }
        if self.circuit_breaker.successes_to_close < 2 {
            return Err(ProviderPolicyError::OutOfRange {
                field: "circuit_breaker.successes_to_close",
            });
        }
        Ok(())
    }

    /// Returns the capped delay before the next attempt, if retry is allowed.
    /// `attempt` is one-based and counts the request that just failed.
    #[must_use]
    pub fn retry_delay_ms(&self, attempt: u8, failure: ProviderFailureKind) -> Option<u64> {
        if !failure.is_retryable() || attempt == 0 || attempt >= self.retry.max_attempts {
            return None;
        }
        let exponent = u32::from(attempt - 1).min(63);
        let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
        Some(
            self.retry
                .initial_backoff_ms
                .saturating_mul(multiplier)
                .min(self.retry.max_backoff_ms),
        )
    }

    #[must_use]
    pub const fn should_open_circuit(&self, consecutive_failures: u32) -> bool {
        consecutive_failures >= self.circuit_breaker.failure_threshold
    }

    #[must_use]
    pub const fn probe_is_available(&self, active_probes: u32) -> bool {
        active_probes < self.circuit_breaker.probe_limit
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderPolicyError {
    #[error("provider policy `{field}` must be positive")]
    NotPositive { field: &'static str },
    #[error("provider policy `{field}` is outside its safe bound")]
    OutOfRange { field: &'static str },
    #[error("provider policy `{field}` must not exceed `{related}`")]
    Inconsistent {
        field: &'static str,
        related: &'static str,
    },
}

/// Durable status values used by `news_ingestion_runs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestionRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NewsValidationError {
    #[error("news field `{field}` must not be blank")]
    Blank { field: &'static str },
    #[error("news field `{field}` contains control characters")]
    ControlCharacters { field: &'static str },
    #[error("news content contains markup; plain text is required")]
    MarkupNotAllowed,
    #[error("news field `{field}` contains unsupported characters")]
    InvalidCharacters { field: &'static str },
    #[error("news field `{field}` exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("news field `{field}` must use an HTTPS URL")]
    InsecureUrl { field: &'static str },
    #[error("news field `{field}` contains URL credentials")]
    UrlCredentials { field: &'static str },
    #[error("news field `{field}` is not normalized")]
    NotNormalized { field: &'static str },
    #[error("source license does not permit feed redistribution")]
    FeedRedistributionNotAllowed,
    #[error("source license approval is not active yet")]
    LicenseNotActive,
    #[error("source license approval has expired")]
    LicenseExpired,
    #[error("source license approval does not match the source")]
    SourceApprovalMismatch,
    #[error("article does not belong to the supplied source")]
    ArticleSourceMismatch,
    #[error("timestamp `{earlier}` cannot be later than `{later}`")]
    TimestampOrder {
        earlier: &'static str,
        later: &'static str,
    },
    #[error("article publication timestamp is in the future")]
    FuturePublicationTimestamp,
    #[error("ingestion run counts cannot be negative")]
    NegativeIngestionCount,
    #[error("ingestion run inserted count cannot exceed seen count")]
    InsertedExceedsSeen,
    #[error("ingestion run status and completion timestamp do not agree")]
    IngestionCompletionMismatch,
    #[error("news error signal attempt must be one-based")]
    InvalidAttempt,
}

fn normalize_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<String, NewsValidationError> {
    if value
        .chars()
        .any(|character| matches!(character, '<' | '>'))
    {
        return Err(NewsValidationError::MarkupNotAllowed);
    }
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(NewsValidationError::Blank { field });
    }
    validate_text(field, &normalized, max)?;
    Ok(normalized)
}

fn normalize_optional_text(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<Option<String>, NewsValidationError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| normalize_text(field, value, max))
        .transpose()
}

fn normalize_identifier(
    field: &'static str,
    value: Option<&str>,
) -> Result<Option<String>, NewsValidationError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(NewsValidationError::InvalidCharacters { field });
    }
    if value.len() > 500 {
        return Err(NewsValidationError::TooLong { field, max: 500 });
    }
    Ok(Some(value.to_owned()))
}

fn normalize_slug(value: &str) -> Result<String, NewsValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NewsValidationError::Blank { field: "slug" });
    }
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric()
            || character == '-'
            || character == '_'
            || character.is_ascii_whitespace())
    }) {
        return Err(NewsValidationError::InvalidCharacters { field: "slug" });
    }

    let mut slug = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        return Err(NewsValidationError::Blank { field: "slug" });
    }
    if slug.len() > 200 {
        return Err(NewsValidationError::TooLong {
            field: "slug",
            max: 200,
        });
    }
    Ok(slug)
}

fn normalize_symbol(value: &str) -> Result<String, NewsValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NewsValidationError::Blank { field: "symbol" });
    }
    if value.len() > 32 {
        return Err(NewsValidationError::TooLong {
            field: "symbol",
            max: 32,
        });
    }
    if !value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '/' | ':' | '_' | '^')
    }) {
        return Err(NewsValidationError::InvalidCharacters { field: "symbol" });
    }
    Ok(value.to_ascii_uppercase())
}

/// Sanitizes provider article content into bounded, plain text.
///
/// Markup is rejected rather than stripped with a best-effort parser. Adapter
/// code must extract plain text before this boundary, so malformed or
/// adversarial HTML cannot be partially interpreted as safe content.
pub fn sanitize_content(value: &str) -> Result<String, NewsValidationError> {
    if value
        .chars()
        .any(|character| matches!(character, '<' | '>'))
    {
        return Err(NewsValidationError::MarkupNotAllowed);
    }
    normalize_text("content", value, 200_000)
}

/// Sanitizes a public URL using the same rules as article and source URLs.
pub fn sanitize_url(value: &str) -> Result<String, NewsValidationError> {
    sanitize_url_field("url", value)
}

fn sanitize_url_field(field: &'static str, value: &str) -> Result<String, NewsValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NewsValidationError::Blank { field });
    }
    if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(NewsValidationError::ControlCharacters { field });
    }

    let lowercase_value = value.to_ascii_lowercase();
    if ["%00", "%0a", "%0d", "%09"]
        .iter()
        .any(|encoded| lowercase_value.contains(encoded))
    {
        return Err(NewsValidationError::ControlCharacters { field });
    }

    let value = value.split_once('#').map_or(value, |(value, _)| value);
    let Some((scheme, authority_and_path)) = value.split_once("://") else {
        return Err(NewsValidationError::InsecureUrl { field });
    };
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(NewsValidationError::InsecureUrl { field });
    }
    let (path, query) = authority_and_path
        .split_once('?')
        .map_or((authority_and_path, None), |(path, query)| {
            (path, Some(query))
        });
    let authority = path.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(NewsValidationError::InvalidCharacters { field });
    }
    if authority.contains('@') {
        return Err(NewsValidationError::UrlCredentials { field });
    }

    let suffix = &path[authority.len()..];
    let suffix = if suffix == "/" { "" } else { suffix };
    let normalized_query = query.map(sanitize_query);
    let normalized = match normalized_query.as_deref() {
        Some(query) if !query.is_empty() => {
            format!(
                "https://{}{}?{query}",
                authority.to_ascii_lowercase(),
                suffix
            )
        }
        _ => format!("https://{}{}", authority.to_ascii_lowercase(), suffix),
    };
    if normalized.len() > 4_000 {
        return Err(NewsValidationError::TooLong { field, max: 4_000 });
    }
    Ok(normalized)
}

fn sanitize_query(query: &str) -> String {
    query
        .split('&')
        .filter(|parameter| {
            let key = parameter
                .split_once('=')
                .map_or(*parameter, |(key, _)| key)
                .to_ascii_lowercase();
            !matches!(
                key.as_str(),
                "utm_source"
                    | "utm_medium"
                    | "utm_campaign"
                    | "utm_term"
                    | "utm_content"
                    | "gclid"
                    | "dclid"
                    | "fbclid"
                    | "msclkid"
                    | "mc_cid"
                    | "mc_eid"
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn normalize_optional_url(
    field: &'static str,
    value: Option<&str>,
) -> Result<Option<String>, NewsValidationError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| sanitize_url_field(field, value))
        .transpose()
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), NewsValidationError> {
    if value.chars().any(char::is_control) {
        return Err(NewsValidationError::ControlCharacters { field });
    }
    if value.len() > max {
        return Err(NewsValidationError::TooLong { field, max });
    }
    Ok(())
}

fn validate_timestamp_order(
    earlier_field: &'static str,
    earlier: DateTime<Utc>,
    later_field: &'static str,
    later: DateTime<Utc>,
) -> Result<(), NewsValidationError> {
    if earlier > later {
        return Err(NewsValidationError::TimestampOrder {
            earlier: earlier_field,
            later: later_field,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type SourceInputCase = (&'static str, fn(&mut NewsSource), NewsValidationError);
    type ArticleInputCase = (&'static str, fn(&mut NewsArticle), NewsValidationError);

    fn source_fixture() -> NewsSource {
        let timestamp = Utc::now();
        NewsSource {
            id: Uuid::nil(),
            name: "Global Wire".to_owned(),
            slug: "global-wire".to_owned(),
            external_id: Some("provider-1".to_owned()),
            source_url: Some("https://example.com/feed".to_owned()),
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    fn article_fixture() -> NewsArticle {
        let timestamp = Utc::now();
        NewsArticle {
            id: Uuid::nil(),
            source_id: Uuid::nil(),
            external_id: Some("provider-1".to_owned()),
            title: "Markets open higher".to_owned(),
            summary: "A market summary".to_owned(),
            content: "Plain article content".to_owned(),
            url: "HTTPS://EXAMPLE.com/article?utm_source=wire#fragment".to_owned(),
            image_url: None,
            author: Some("Analyst".to_owned()),
            category: Some("markets".to_owned()),
            symbols: vec!["AAPL".to_owned()],
            published_at: timestamp,
            ingested_at: timestamp,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    #[test]
    fn source_normalization_is_canonical() {
        let mut source = NewsSource {
            id: Uuid::nil(),
            name: "  Global   Wire  ".to_owned(),
            slug: "Global_Wire".to_owned(),
            external_id: Some(" provider-1 ".to_owned()),
            source_url: Some(
                "HTTPS://EXAMPLE.com/feed?utm_source=wire&edition=global#fragment".to_owned(),
            ),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        source.normalize().expect("valid source");

        assert_eq!(source.name, "Global Wire");
        assert_eq!(source.slug, "global-wire");
        assert_eq!(source.external_id.as_deref(), Some("provider-1"));
        assert_eq!(
            source.source_url.as_deref(),
            Some("https://example.com/feed?edition=global")
        );
        assert!(source.validate().is_ok());
    }

    #[test]
    fn normalization_is_idempotent() {
        let mut source = source_fixture();
        source.normalize().expect("valid source");
        let normalized_source = source.clone();
        source.normalize().expect("already normalized source");
        assert_eq!(source, normalized_source);

        let mut article = article_fixture();
        article.normalize().expect("valid article");
        let normalized_article = article.clone();
        article.normalize().expect("already normalized article");
        assert_eq!(article, normalized_article);
    }

    #[test]
    fn malicious_source_inputs_are_rejected() {
        let cases: [SourceInputCase; 5] = [
            (
                "control character in name",
                |source: &mut NewsSource| source.name = "trusted\0source".to_owned(),
                NewsValidationError::ControlCharacters { field: "name" },
            ),
            (
                "path traversal in slug",
                |source: &mut NewsSource| source.slug = "../admin".to_owned(),
                NewsValidationError::InvalidCharacters { field: "slug" },
            ),
            (
                "insecure source URL",
                |source: &mut NewsSource| {
                    source.source_url = Some("http://example.com/feed".to_owned());
                },
                NewsValidationError::InsecureUrl {
                    field: "source_url",
                },
            ),
            (
                "URL credentials",
                |source: &mut NewsSource| {
                    source.source_url = Some("https://user:pass@example.com/feed".to_owned());
                },
                NewsValidationError::UrlCredentials {
                    field: "source_url",
                },
            ),
            (
                "encoded line break in URL",
                |source: &mut NewsSource| {
                    source.source_url = Some("https://example.com/feed%0d".to_owned());
                },
                NewsValidationError::ControlCharacters {
                    field: "source_url",
                },
            ),
        ];

        for (case, mutate, expected) in cases {
            let mut source = source_fixture();
            mutate(&mut source);
            assert_eq!(source.normalize(), Err(expected), "{case}");
        }
    }

    #[test]
    fn malicious_article_inputs_are_rejected() {
        let cases: [ArticleInputCase; 7] = [
            (
                "HTML event handler",
                |article: &mut NewsArticle| {
                    article.content = "<img src=x onerror=alert(1)>".to_owned();
                },
                NewsValidationError::MarkupNotAllowed,
            ),
            (
                "HTML in title",
                |article: &mut NewsArticle| {
                    article.title = "<script>alert(1)</script>".to_owned();
                },
                NewsValidationError::MarkupNotAllowed,
            ),
            (
                "control character in title",
                |article: &mut NewsArticle| article.title = "headline\0".to_owned(),
                NewsValidationError::ControlCharacters { field: "title" },
            ),
            (
                "insecure article URL",
                |article: &mut NewsArticle| article.url = "javascript:alert(1)".to_owned(),
                NewsValidationError::InsecureUrl { field: "url" },
            ),
            (
                "malicious symbol characters",
                |article: &mut NewsArticle| article.symbols = vec!["AAPL<script>".to_owned()],
                NewsValidationError::InvalidCharacters { field: "symbol" },
            ),
            (
                "whitespace in external ID",
                |article: &mut NewsArticle| {
                    article.external_id = Some("provider\nsecret".to_owned());
                },
                NewsValidationError::InvalidCharacters {
                    field: "external_id",
                },
            ),
            (
                "oversized title",
                |article: &mut NewsArticle| {
                    article.title = "x".repeat(501);
                },
                NewsValidationError::TooLong {
                    field: "title",
                    max: 500,
                },
            ),
        ];

        for (case, mutate, expected) in cases {
            let mut article = article_fixture();
            mutate(&mut article);
            assert_eq!(article.normalize(), Err(expected), "{case}");
        }
    }

    #[test]
    fn equivalent_canonical_urls_share_deduplication_identity() {
        let mut first = article_fixture();
        first.url = "HTTPS://EXAMPLE.com/article?utm_source=wire#top".to_owned();
        first.normalize().expect("first URL is safe");

        let mut second = article_fixture();
        second.url = "https://example.com/article".to_owned();
        second.normalize().expect("second URL is safe");

        assert_eq!(
            first.deduplication_identity().expect("first identity"),
            second.deduplication_identity().expect("second identity")
        );
    }

    #[test]
    fn article_normalization_deduplicates_symbols() {
        let timestamp = Utc::now();
        let mut article = NewsArticle {
            id: Uuid::nil(),
            source_id: Uuid::nil(),
            external_id: Some("provider-7".to_owned()),
            title: "  Markets   open  ".to_owned(),
            summary: "A   concise summary".to_owned(),
            content: "Plain   article content".to_owned(),
            url: "https://example.com/article".to_owned(),
            image_url: None,
            author: Some("  Analyst  ".to_owned()),
            category: Some("Global Markets".to_owned()),
            symbols: vec!["aapl".to_owned(), "EURUSD".to_owned(), "AAPL".to_owned()],
            published_at: timestamp,
            ingested_at: timestamp,
            created_at: timestamp,
            updated_at: timestamp,
        };

        article.normalize().expect("valid article");

        assert_eq!(article.title, "Markets open");
        assert_eq!(article.category.as_deref(), Some("global markets"));
        assert_eq!(
            article.symbols,
            vec!["AAPL".to_owned(), "EURUSD".to_owned()]
        );
        assert_eq!(
            article.deduplication_identity().expect("stable identity"),
            NewsDeduplicationIdentity {
                primary: NewsDeduplicationKey::CanonicalUrl(
                    "https://example.com/article".to_owned()
                ),
                fallback: Some(NewsDeduplicationKey::SourceExternalId {
                    source_id: Uuid::nil(),
                    external_id: "provider-7".to_owned(),
                }),
            }
        );
        let freshness = article
            .freshness_at(
                timestamp + chrono::Duration::minutes(30),
                &NewsFreshnessPolicy::default(),
            )
            .expect("valid freshness signal");
        assert_eq!(freshness.status, NewsFreshness::Aging);
        assert_eq!(
            article
                .freshness_at(
                    timestamp - chrono::Duration::milliseconds(1),
                    &NewsFreshnessPolicy::default(),
                )
                .expect("future freshness signal")
                .status,
            NewsFreshness::FutureDated
        );
        assert!(article.validate().is_ok());
    }

    #[test]
    fn lifecycle_transitions_are_ordered() {
        assert!(ArticleLifecycle::Fetched.can_advance_to(ArticleLifecycle::Normalized));
        assert!(!ArticleLifecycle::Validated.can_advance_to(ArticleLifecycle::Persisted));
        assert!(ProviderHealth::CircuitOpen.can_transition_to(ProviderHealth::Probing));
        assert!(!ProviderHealth::Healthy.can_transition_to(ProviderHealth::Probing));
    }

    #[test]
    fn provider_policy_bounds_retries_and_opens_circuits() {
        let policy = ProviderRuntimePolicy::default();
        assert!(policy.validate().is_ok());
        assert_eq!(
            policy.retry_delay_ms(1, ProviderFailureKind::Timeout),
            Some(250)
        );
        assert_eq!(
            policy.retry_delay_ms(2, ProviderFailureKind::UpstreamServer),
            Some(500)
        );
        assert_eq!(
            policy.retry_delay_ms(3, ProviderFailureKind::RateLimited),
            None
        );
        assert_eq!(
            policy.retry_delay_ms(1, ProviderFailureKind::InvalidPayload),
            None
        );
        assert!(policy.should_open_circuit(5));
        assert!(policy.probe_is_available(0));
        assert!(!policy.probe_is_available(1));

        let mut invalid = policy;
        invalid.timeout.request_timeout_ms = 10_001;
        assert_eq!(
            invalid.validate(),
            Err(ProviderPolicyError::OutOfRange {
                field: "timeout.request_timeout_ms",
            })
        );
    }

    #[test]
    fn error_signals_expose_retryability_without_raw_errors() {
        let signal = NewsErrorSignal {
            source_id: Uuid::nil(),
            article_id: None,
            kind: NewsErrorSignalKind::Provider(ProviderFailureKind::Transport),
            occurred_at: Utc::now(),
            attempt: 1,
        };
        let policy = ProviderRuntimePolicy::default();

        assert!(signal.is_retryable());
        assert!(signal.validate().is_ok());
        assert_eq!(signal.retry_delay_ms(&policy), Some(250));

        let invalid = NewsErrorSignal {
            kind: NewsErrorSignalKind::NormalizationRejected,
            ..signal
        };
        assert!(!invalid.is_retryable());
        assert_eq!(invalid.retry_delay_ms(&policy), None);

        let invalid_attempt = NewsErrorSignal {
            attempt: 0,
            ..signal
        };
        assert_eq!(
            invalid_attempt.validate(),
            Err(NewsValidationError::InvalidAttempt)
        );
    }

    #[test]
    fn sanitizers_reject_markup_and_encoded_controls() {
        assert_eq!(
            sanitize_content("<script>alert(1)</script>"),
            Err(NewsValidationError::MarkupNotAllowed)
        );
        assert_eq!(
            sanitize_url("https://example.com/article%0a"),
            Err(NewsValidationError::ControlCharacters { field: "url" })
        );
        assert_eq!(
            sanitize_url("HTTPS://EXAMPLE.com/article?utm_medium=email#top").expect("safe URL"),
            "https://example.com/article"
        );
        assert_eq!(
            sanitize_url("https://example.com/").expect("safe root URL"),
            sanitize_url("https://example.com").expect("safe host URL")
        );
    }

    #[test]
    fn publication_requires_active_redistribution_approval() {
        let now = Utc::now();
        let source_id = Uuid::from_u128(1);
        let source = NewsSource {
            id: source_id,
            name: "Global Wire".to_owned(),
            slug: "global-wire".to_owned(),
            external_id: None,
            source_url: Some("https://example.com/feed".to_owned()),
            created_at: now,
            updated_at: now,
        };
        let article = NewsArticle {
            id: Uuid::from_u128(2),
            source_id,
            external_id: None,
            title: "Markets open higher".to_owned(),
            summary: "A market summary".to_owned(),
            content: "Plain article content".to_owned(),
            url: "https://example.com/article".to_owned(),
            image_url: None,
            author: None,
            category: Some("markets".to_owned()),
            symbols: vec!["AAPL".to_owned()],
            published_at: now,
            ingested_at: now,
            created_at: now,
            updated_at: now,
        };
        let approval = SourceLicenseApproval {
            source_id,
            license_name: "Provider agreement".to_owned(),
            license_url: "https://example.com/license".to_owned(),
            feed_redistribution_allowed: true,
            attribution: AttributionPolicy::SourceAndArticleLink,
            approved_at: now - chrono::Duration::hours(1),
            expires_at: Some(now + chrono::Duration::hours(1)),
        };

        let attribution = article
            .attribution(&source, &approval, now)
            .expect("active approval permits publication");
        assert_eq!(attribution.source_name, "Global Wire");
        assert_eq!(attribution.article_url, article.url);

        let mut denied = approval;
        denied.feed_redistribution_allowed = false;
        assert_eq!(
            article.attribution(&source, &denied, now),
            Err(NewsValidationError::FeedRedistributionNotAllowed)
        );
    }

    #[test]
    fn ingestion_run_requires_utc_ordered_completion() {
        let started_at = Utc::now();
        let run = NewsIngestionRun {
            id: Uuid::nil(),
            source_id: Uuid::nil(),
            started_at,
            completed_at: Some(started_at + chrono::Duration::minutes(1)),
            status: IngestionRunStatus::Completed,
            articles_seen: 2,
            articles_inserted: 1,
            error_message: None,
        };

        assert!(run.is_completed());
        assert!(run.validate().is_ok());

        let invalid = NewsIngestionRun {
            completed_at: Some(started_at - chrono::Duration::minutes(1)),
            ..run
        };
        assert_eq!(
            invalid.validate(),
            Err(NewsValidationError::TimestampOrder {
                earlier: "started_at",
                later: "completed_at",
            })
        );
    }

    #[test]
    fn provider_contract_rejects_missing_required_fields() {
        let article = serde_json::json!({
            "id": Uuid::nil(),
            "source_id": Uuid::nil(),
            "external_id": null,
            "summary": "A market summary",
            "content": "Plain article content",
            "url": "https://example.com/article",
            "image_url": null,
            "author": null,
            "category": null,
            "symbols": [],
            "published_at": "2026-08-09T12:00:00Z",
            "ingested_at": "2026-08-09T12:00:00Z",
            "created_at": "2026-08-09T12:00:00Z",
            "updated_at": "2026-08-09T12:00:00Z"
        });
        assert!(serde_json::from_value::<NewsArticle>(article).is_err());

        let source = serde_json::json!({
            "id": Uuid::nil(),
            "slug": "global-wire",
            "external_id": null,
            "source_url": null,
            "created_at": "2026-08-09T12:00:00Z",
            "updated_at": "2026-08-09T12:00:00Z"
        });
        assert!(serde_json::from_value::<NewsSource>(source).is_err());

        let run = serde_json::json!({
            "id": Uuid::nil(),
            "source_id": Uuid::nil(),
            "started_at": "2026-08-09T12:00:00Z",
            "completed_at": null,
            "articles_seen": 0,
            "articles_inserted": 0,
            "error_message": null
        });
        assert!(serde_json::from_value::<NewsIngestionRun>(run).is_err());
    }

    #[test]
    fn provider_contract_converts_offsets_to_utc_and_rejects_naive_time() {
        let offset: DateTime<Utc> = serde_json::from_str("\"2026-08-09T17:30:00+05:30\"")
            .expect("RFC 3339 offsets are accepted");
        assert_eq!(
            offset,
            "2026-08-09T12:00:00Z"
                .parse::<DateTime<Utc>>()
                .expect("UTC timestamp")
        );
        assert_eq!(
            serde_json::to_string(&offset).expect("UTC serialization"),
            "\"2026-08-09T12:00:00Z\""
        );

        let naive = serde_json::from_str::<DateTime<Utc>>("\"2026-08-09T12:00:00\"");
        assert!(naive.is_err());
    }
}
