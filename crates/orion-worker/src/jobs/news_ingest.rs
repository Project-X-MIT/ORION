//! Multi-source market-news ingestion.
//!
//! This module deliberately owns provider adapters and worker orchestration,
//! not the normalized news contract. The domain contract is currently not
//! exported by `orion-domain/src/lib.rs` because that file is Div-owned. Once
//! Div publishes it, the conversion boundary in [`normalize_article`] must use
//! `orion_domain::news::NewsArticle` and its validation methods directly.
//!
//! The shared outbox writer/transaction API is not present in this checkout.
//! [`NewsOutbox`] is the explicit seam for that dependency. The worker uses
//! the completed DB-05 idempotent article upsert and fails closed if the
//! publication event cannot be enqueued; Div's transaction-aware outbox API
//! must later combine those two operations atomically.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use orion_db::{
    models::{NewsArticle, NewsSource},
    queries::news::upsert_article,
};
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::{debug, warn};
use uuid::Uuid;

const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARTICLES_PER_FEED: usize = 500;
const MAX_ERROR_CODE_BYTES: usize = 200;

/// A boxed future keeps the provider and outbox seams usable without adding
/// an async-trait dependency to the worker crate.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One request made by an adapter. The URL is copied so an implementation can
/// safely hand it to an HTTP client without retaining a database row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequest {
    pub url: String,
    pub timeout: Duration,
    pub attempt: u8,
    pub user_agent: String,
}

/// Minimal response returned by the HTTP boundary. Body parsing stays inside
/// the adapter and never enters the domain or persistence layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderResponse {
    pub status: u16,
    pub body: String,
}

/// HTTP implementation owned by the platform/provider integration team.
///
/// The worker does not construct a client from environment variables. Div's
/// configuration registry must supply the approved client/user-agent and the
/// adapter must enforce the request timeout before this method returns.
pub trait NewsTransport: Send + Sync {
    fn get<'a>(&'a self, request: ProviderRequest) -> BoxFuture<'a, Result<ProviderResponse>>;
}

/// Provider-neutral, untrusted item returned by an adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderArticle {
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub url: Option<String>,
    pub image_url: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub symbols: Vec<String>,
    pub published_at: Option<String>,
}

/// A provider response after parsing, but before normalization and validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderFeed {
    pub articles: Vec<ProviderArticle>,
}

/// Safe provider failure classes. Raw response bodies, URLs containing query
/// secrets, and transport error strings are intentionally not retained.
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

/// Adapter failures are intentionally safe to persist and alert on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapterError {
    pub kind: ProviderFailureKind,
    pub code: &'static str,
}

impl ProviderAdapterError {
    const fn new(kind: ProviderFailureKind, code: &'static str) -> Self {
        Self { kind, code }
    }

    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        self.kind.is_retryable()
    }
}

/// Provider adapter contract consumed by the worker. Provider-specific
/// response types belong in the adapter implementation, never in the domain.
pub trait NewsProviderAdapter: Send + Sync {
    fn fetch<'a>(
        &'a self,
        source: &'a NewsSource,
        request: ProviderRequest,
    ) -> BoxFuture<'a, std::result::Result<ProviderFeed, ProviderAdapterError>>;
}

/// Generic RSS 2.x/Atom adapter used by approved feeds. It does not assume
/// that all feeds have the same optional fields; only title, URL, publication
/// time and at least one body field are required by normalization.
pub struct RssProviderAdapter {
    transport: Arc<dyn NewsTransport>,
    user_agent: String,
}

impl RssProviderAdapter {
    #[must_use]
    pub fn new(transport: Arc<dyn NewsTransport>, user_agent: impl Into<String>) -> Self {
        Self {
            transport,
            user_agent: user_agent.into(),
        }
    }

    /// Parses a bounded RSS/Atom document without entity expansion. This is a
    /// pure function so malformed-feed fixtures can test the adapter without
    /// an HTTP client or PostgreSQL.
    pub fn parse_feed(body: &str) -> std::result::Result<ProviderFeed, ProviderAdapterError> {
        if body.len() > MAX_FEED_BYTES {
            return Err(ProviderAdapterError::new(
                ProviderFailureKind::InvalidPayload,
                "feed_body_too_large",
            ));
        }
        if body.contains('\0')
            || contains_ascii_case_insensitive(body, "<!doctype")
            || contains_ascii_case_insensitive(body, "<!entity")
        {
            return Err(ProviderAdapterError::new(
                ProviderFailureKind::InvalidPayload,
                "unsafe_xml_declaration",
            ));
        }

        let blocks = extract_blocks(body, "item")
            .or_else(|| extract_blocks(body, "entry"))
            .ok_or_else(|| {
                ProviderAdapterError::new(ProviderFailureKind::InvalidPayload, "missing_feed_items")
            })?;

        if blocks.len() > MAX_ARTICLES_PER_FEED {
            return Err(ProviderAdapterError::new(
                ProviderFailureKind::InvalidPayload,
                "too_many_feed_items",
            ));
        }

        let articles = blocks
            .into_iter()
            .map(parse_article_block)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ProviderFeed { articles })
    }
}

impl NewsProviderAdapter for RssProviderAdapter {
    fn fetch<'a>(
        &'a self,
        source: &'a NewsSource,
        mut request: ProviderRequest,
    ) -> BoxFuture<'a, std::result::Result<ProviderFeed, ProviderAdapterError>> {
        Box::pin(async move {
            if source.source_url.is_none() || request.url.is_empty() {
                return Err(ProviderAdapterError::new(
                    ProviderFailureKind::InvalidPayload,
                    "source_url_missing",
                ));
            }
            request.url = canonicalize_url("source_url", Some(&request.url)).map_err(|_| {
                ProviderAdapterError::new(ProviderFailureKind::InvalidPayload, "source_url_invalid")
            })?;
            request.user_agent = self.user_agent.clone();
            let response = self.transport.get(request).await.map_err(|_| {
                ProviderAdapterError::new(ProviderFailureKind::Transport, "transport_failure")
            })?;
            classify_http_response(response, source)
        })
    }
}

fn classify_http_response(
    response: ProviderResponse,
    _source: &NewsSource,
) -> std::result::Result<ProviderFeed, ProviderAdapterError> {
    match response.status {
        200..=299 => RssProviderAdapter::parse_feed(&response.body),
        401 | 403 => Err(ProviderAdapterError::new(
            ProviderFailureKind::Unauthorized,
            "provider_unauthorized",
        )),
        408 | 429 => Err(ProviderAdapterError::new(
            if response.status == 429 {
                ProviderFailureKind::RateLimited
            } else {
                ProviderFailureKind::Timeout
            },
            if response.status == 429 {
                "provider_rate_limited"
            } else {
                "provider_timeout"
            },
        )),
        500..=599 => Err(ProviderAdapterError::new(
            ProviderFailureKind::UpstreamServer,
            "provider_server_error",
        )),
        _ => Err(ProviderAdapterError::new(
            ProviderFailureKind::InvalidPayload,
            "provider_http_status",
        )),
    }
}

/// Normalizes an untrusted provider item to the DB-05 article shape.
///
/// This is the temporary compatibility conversion described at the top of
/// this file. It intentionally mirrors the domain safety rules: HTTPS-only
/// URLs, bounded plain text, UTC timestamps, canonical symbols and no markup.
pub fn normalize_article(
    source_id: Uuid,
    raw: ProviderArticle,
    observed_at: DateTime<Utc>,
) -> std::result::Result<NewsArticle, NormalizationError> {
    let title = required_text("title", raw.title.as_deref())?;
    let url = canonicalize_url("url", raw.url.as_deref())?;
    let published_at = parse_utc_timestamp(raw.published_at.as_deref())?;

    let summary = optional_plain_text("summary", raw.summary.as_deref())?;
    let content = optional_plain_text("content", raw.content.as_deref())?;
    let (summary, content) = match (summary, content) {
        (Some(summary), Some(content)) => (summary, content),
        (Some(summary), None) => (summary.clone(), summary),
        (None, Some(content)) => (content.clone(), content),
        (None, None) => return Err(NormalizationError::MissingField("content")),
    };

    let external_id = normalize_identifier(raw.external_id.as_deref())?;
    let image_url = raw
        .image_url
        .as_deref()
        .map(|value| canonicalize_url("image_url", Some(value)))
        .transpose()?;
    let author = raw
        .author
        .as_deref()
        .map(|value| optional_plain_text("author", Some(value)))
        .transpose()?
        .flatten();
    let category = raw
        .category
        .as_deref()
        .map(|value| optional_plain_text("category", Some(value)))
        .transpose()?
        .flatten()
        .map(|value| value.to_ascii_lowercase());
    let symbols = normalize_symbols(raw.symbols)?;

    let article = NewsArticle {
        id: Uuid::now_v7(),
        source_id,
        external_id,
        title,
        summary,
        content,
        url,
        image_url,
        author,
        category,
        symbols,
        published_at,
        ingested_at: observed_at,
        created_at: observed_at,
        updated_at: observed_at,
    };
    validate_article(&article)?;
    Ok(article)
}

/// Validates the normalized DB-05 article shape immediately before
/// persistence. This remains a worker-local compatibility check until Div
/// exports the canonical `orion_domain::news::NewsArticle::validate` method.
pub fn validate_article(article: &NewsArticle) -> std::result::Result<(), NormalizationError> {
    if required_text("title", Some(&article.title))? != article.title {
        return Err(NormalizationError::NotNormalized("title"));
    }
    if required_text("summary", Some(&article.summary))? != article.summary {
        return Err(NormalizationError::NotNormalized("summary"));
    }
    if required_text("content", Some(&article.content))? != article.content {
        return Err(NormalizationError::NotNormalized("content"));
    }
    if canonicalize_url("url", Some(&article.url))? != article.url {
        return Err(NormalizationError::NotNormalized("url"));
    }
    if let Some(image_url) = &article.image_url {
        if canonicalize_url("image_url", Some(image_url))? != *image_url {
            return Err(NormalizationError::NotNormalized("image_url"));
        }
    }
    if normalize_identifier(article.external_id.as_deref())? != article.external_id {
        return Err(NormalizationError::NotNormalized("external_id"));
    }
    if optional_plain_text("author", article.author.as_deref())? != article.author {
        return Err(NormalizationError::NotNormalized("author"));
    }
    let normalized_category = optional_plain_text("category", article.category.as_deref())?
        .map(|category| category.to_ascii_lowercase());
    if normalized_category != article.category {
        return Err(NormalizationError::NotNormalized("category"));
    }
    if normalize_symbols(article.symbols.clone())? != article.symbols {
        return Err(NormalizationError::NotNormalized("symbols"));
    }
    if article.ingested_at < article.created_at || article.updated_at < article.ingested_at {
        return Err(NormalizationError::TimestampOrder);
    }
    Ok(())
}

/// Stable key used by the worker when emitting idempotent outbox messages.
/// PostgreSQL remains authoritative for the URL/external-ID upsert identity.
#[must_use]
pub fn article_event_key(article: &NewsArticle) -> String {
    format!("news.article.published:{}", article.url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationError {
    MissingField(&'static str),
    BlankField(&'static str),
    UnsafeText(&'static str),
    InvalidUrl(&'static str),
    InvalidIdentifier,
    InvalidSymbol,
    InvalidTimestamp,
    TooLong(&'static str),
    InvalidFeed,
    NotNormalized(&'static str),
    TimestampOrder,
}

/// Source/license admission belongs to the configuration/product approval
/// owner. The worker fails closed when this dependency rejects a source.
pub trait NewsSourceAdmission: Send + Sync {
    fn check(
        &self,
        source: &NewsSource,
        at: DateTime<Utc>,
    ) -> std::result::Result<(), &'static str>;
}

/// Shared outbox seam. DB-05 article persistence is performed by the
/// completed [`upsert_article`] query before this event is enqueued. Once Div
/// lands a transaction-aware outbox API, this method should move into that
/// shared transaction boundary.
pub trait NewsOutbox: Send + Sync {
    fn enqueue_article_published<'a>(
        &'a self,
        article: &'a NewsArticle,
        event_key: &'a str,
    ) -> BoxFuture<'a, Result<()>>;

    fn enqueue_terminal_alert<'a>(
        &'a self,
        source_id: Uuid,
        code: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
}

/// Invalidates the registered `cache.news_feed` key family after a committed
/// article change. The implementation belongs to the Redis/platform owner;
/// this worker never invents raw Redis keys.
pub trait NewsCacheInvalidator: Send + Sync {
    fn invalidate_news_feed<'a>(&'a self, source_id: Uuid) -> BoxFuture<'a, Result<()>>;
}

/// Safe default for local/unit usage. It is intentionally not a production
/// implementation; production startup should inject Div's outbox writer.
pub struct UnconfiguredNewsOutbox;

impl NewsOutbox for UnconfiguredNewsOutbox {
    fn enqueue_article_published<'a>(
        &'a self,
        _article: &'a NewsArticle,
        _event_key: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Err(anyhow!("shared outbox is not configured")) })
    }

    fn enqueue_terminal_alert<'a>(
        &'a self,
        _source_id: Uuid,
        _code: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Err(anyhow!("shared outbox is not configured")) })
    }
}

/// Placeholder until the shared Redis invalidation API is available. Cache
/// failure is deliberately non-fatal because Redis is disposable and the
/// public feed falls back to PostgreSQL.
pub struct UnconfiguredNewsCacheInvalidator;

impl NewsCacheInvalidator for UnconfiguredNewsCacheInvalidator {
    fn invalidate_news_feed<'a>(&'a self, _source_id: Uuid) -> BoxFuture<'a, Result<()>> {
        Box::pin(async { Err(anyhow!("news feed cache invalidator is not configured")) })
    }
}

/// Bounded worker policy aligned with the domain provider policy. Values are
/// copied here only until the domain module is publicly exported to workers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionPolicy {
    pub request_timeout_ms: u64,
    pub max_requests_per_window: u32,
    pub window_seconds: u32,
    pub burst_capacity: u32,
    pub max_attempts: u8,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub failure_threshold: u32,
    pub open_for_seconds: u64,
    pub probe_limit: u32,
    pub successes_to_close: u32,
}

impl Default for IngestionPolicy {
    fn default() -> Self {
        Self {
            request_timeout_ms: 10_000,
            max_requests_per_window: 60,
            window_seconds: 60,
            burst_capacity: 5,
            max_attempts: 3,
            initial_backoff_ms: 250,
            max_backoff_ms: 4_000,
            failure_threshold: 5,
            open_for_seconds: 60,
            probe_limit: 1,
            successes_to_close: 2,
        }
    }
}

impl IngestionPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.request_timeout_ms == 0
            || self.request_timeout_ms > 10_000
            || self.max_requests_per_window == 0
            || self.window_seconds == 0
            || self.max_requests_per_window > self.window_seconds
            || self.burst_capacity == 0
            || self.burst_capacity > self.max_requests_per_window
            || self.max_attempts == 0
            || self.max_attempts > 3
            || self.initial_backoff_ms == 0
            || self.max_backoff_ms < self.initial_backoff_ms
            || self.max_backoff_ms > 4_000
            || self.failure_threshold == 0
            || self.open_for_seconds < 60
            || self.probe_limit != 1
            || self.successes_to_close < 2
        {
            return Err(anyhow!("news ingestion policy is outside safe bounds"));
        }
        Ok(())
    }

    #[must_use]
    pub fn retry_delay(&self, attempt: u8, kind: ProviderFailureKind) -> Option<Duration> {
        if !kind.is_retryable() || attempt == 0 || attempt >= self.max_attempts {
            return None;
        }
        let multiplier = 1_u64
            .checked_shl(u32::from(attempt - 1))
            .unwrap_or(u64::MAX);
        Some(Duration::from_millis(
            self.initial_backoff_ms
                .saturating_mul(multiplier)
                .min(self.max_backoff_ms),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitHealth {
    Healthy,
    Degraded,
    CircuitOpen,
    Probing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRunStatus {
    Completed,
    Failed,
    CircuitOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRunOutcome {
    pub source_id: Uuid,
    pub status: SourceRunStatus,
    pub articles_seen: u32,
    pub articles_inserted: u32,
    pub articles_rejected: u32,
    pub attempts: u8,
    pub terminal_code: Option<&'static str>,
}

impl SourceRunOutcome {
    /// Returns the number of fetched items that were not committed by the
    /// run. DB-05 stores `articles_seen` and `articles_inserted`; the
    /// persisted error count is therefore their deterministic difference.
    #[must_use]
    pub const fn error_count(&self) -> u32 {
        self.articles_seen.saturating_sub(self.articles_inserted)
    }

    /// Confirms that the run's persisted counts are internally consistent.
    /// `articles_rejected` is a normalization-error subset of the total
    /// non-inserted count and may be smaller for a terminal provider/DB error.
    #[must_use]
    pub const fn counts_reconcile(&self) -> bool {
        self.articles_inserted <= self.articles_seen && self.articles_rejected <= self.error_count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchRunOutcome {
    pub sources_seen: u32,
    pub sources_completed: u32,
    pub sources_failed: u32,
    pub source_results: Vec<SourceRunOutcome>,
}

struct ProviderState {
    health: CircuitHealth,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    active_probes: u32,
    probe_successes: u32,
    request_times: VecDeque<Instant>,
    burst_times: VecDeque<Instant>,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self {
            health: CircuitHealth::Healthy,
            consecutive_failures: 0,
            opened_at: None,
            active_probes: 0,
            probe_successes: 0,
            request_times: VecDeque::new(),
            burst_times: VecDeque::new(),
        }
    }
}

/// Coordinates source-independent fetch, retry, normalization and persistence.
pub struct NewsIngestor {
    pool: PgPool,
    adapter: Arc<dyn NewsProviderAdapter>,
    admission: Arc<dyn NewsSourceAdmission>,
    outbox: Arc<dyn NewsOutbox>,
    cache_invalidator: Arc<dyn NewsCacheInvalidator>,
    policy: IngestionPolicy,
    providers: Mutex<HashMap<Uuid, ProviderState>>,
}

impl NewsIngestor {
    pub fn new(
        pool: PgPool,
        adapter: Arc<dyn NewsProviderAdapter>,
        admission: Arc<dyn NewsSourceAdmission>,
        outbox: Arc<dyn NewsOutbox>,
        policy: IngestionPolicy,
    ) -> Result<Self> {
        Self::new_with_cache_invalidator(
            pool,
            adapter,
            admission,
            outbox,
            Arc::new(UnconfiguredNewsCacheInvalidator),
            policy,
        )
    }

    pub fn new_with_cache_invalidator(
        pool: PgPool,
        adapter: Arc<dyn NewsProviderAdapter>,
        admission: Arc<dyn NewsSourceAdmission>,
        outbox: Arc<dyn NewsOutbox>,
        cache_invalidator: Arc<dyn NewsCacheInvalidator>,
        policy: IngestionPolicy,
    ) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            pool,
            adapter,
            admission,
            outbox,
            cache_invalidator,
            policy,
            providers: Mutex::new(HashMap::new()),
        })
    }

    /// Runs every admitted source independently. A source error becomes a
    /// source result and terminal alert; it never aborts the source loop.
    pub async fn run_once(&self, observed_at: DateTime<Utc>) -> Result<BatchRunOutcome> {
        // TODO: replace with an orion-db repository/query call once DIV-08/DIV-12 lands.
        let sources = sqlx::query_as::<_, NewsSource>(
            "SELECT id, name, slug, external_id, source_url, created_at, updated_at \
             FROM news_sources ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| anyhow!("news source discovery failed"))?;

        let mut outcome = BatchRunOutcome {
            sources_seen: u32::try_from(sources.len()).unwrap_or(u32::MAX),
            ..BatchRunOutcome::default()
        };
        for source in sources {
            let result = self.ingest_source(&source, observed_at).await;
            match result.status {
                SourceRunStatus::Completed => outcome.sources_completed += 1,
                SourceRunStatus::Failed | SourceRunStatus::CircuitOpen => {
                    outcome.sources_failed += 1
                }
            }
            outcome.source_results.push(result);
        }
        Ok(outcome)
    }

    async fn ingest_source(
        &self,
        source: &NewsSource,
        observed_at: DateTime<Utc>,
    ) -> SourceRunOutcome {
        let run_id = match start_run(&self.pool, source.id, observed_at).await {
            Ok(id) => id,
            Err(_) => {
                self.alert_terminal(source.id, "run_start_failed").await;
                return failed_outcome(source.id, "run_start_failed");
            }
        };

        if let Err(code) = self.admission.check(source, observed_at) {
            let code = bounded_code(code);
            let _ = finish_run(
                &self.pool,
                run_id,
                SourceRunStatus::Failed,
                observed_at,
                0,
                0,
                Some(code),
            )
            .await;
            self.alert_terminal(source.id, code).await;
            return failed_outcome(source.id, code);
        }

        let mut attempts = 0;
        let mut last_error = None;
        for attempt in 1..=self.policy.max_attempts {
            attempts = attempt;
            let probe = match self.before_request(source.id) {
                Ok(probe) => probe,
                Err(_) => {
                    let _ = finish_run(
                        &self.pool,
                        run_id,
                        SourceRunStatus::CircuitOpen,
                        observed_at,
                        0,
                        0,
                        Some("circuit_open"),
                    )
                    .await;
                    self.alert_terminal(source.id, "circuit_open").await;
                    return SourceRunOutcome {
                        source_id: source.id,
                        status: SourceRunStatus::CircuitOpen,
                        articles_seen: 0,
                        articles_inserted: 0,
                        articles_rejected: 0,
                        attempts,
                        terminal_code: Some("circuit_open"),
                    };
                }
            };

            self.wait_for_rate_slot(source.id).await;
            let request = ProviderRequest {
                url: source.source_url.clone().unwrap_or_default(),
                timeout: Duration::from_millis(self.policy.request_timeout_ms),
                attempt,
                user_agent: "ORION-news-ingest/1.0".to_owned(),
            };
            let fetched =
                match tokio::time::timeout(request.timeout, self.adapter.fetch(source, request))
                    .await
                {
                    Ok(result) => result,
                    Err(_) => Err(ProviderAdapterError::new(
                        ProviderFailureKind::Timeout,
                        "provider_timeout",
                    )),
                };
            match fetched {
                Ok(feed) => {
                    self.record_success(source.id, probe);
                    let result = self
                        .persist_feed(source.id, run_id, feed, observed_at)
                        .await;
                    if result.status != SourceRunStatus::Completed {
                        self.alert_terminal(
                            source.id,
                            result.terminal_code.unwrap_or("persistence_failed"),
                        )
                        .await;
                    }
                    return SourceRunOutcome { attempts, ..result };
                }
                Err(error) => {
                    let retryable = error.is_retryable();
                    let retry_delay = self.policy.retry_delay(attempt, error.kind);
                    self.record_failure(source.id, probe);
                    last_error = Some(error);
                    if !retryable {
                        break;
                    }
                    if let Some(delay) = retry_delay {
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        let code = last_error.map_or("provider_failure", |error| error.code);
        let _ = finish_run(
            &self.pool,
            run_id,
            SourceRunStatus::Failed,
            observed_at,
            0,
            0,
            Some(code),
        )
        .await;
        self.alert_terminal(source.id, code).await;
        SourceRunOutcome {
            source_id: source.id,
            status: SourceRunStatus::Failed,
            articles_seen: 0,
            articles_inserted: 0,
            articles_rejected: 0,
            attempts,
            terminal_code: Some(code),
        }
    }

    async fn persist_feed(
        &self,
        source_id: Uuid,
        run_id: Uuid,
        feed: ProviderFeed,
        observed_at: DateTime<Utc>,
    ) -> SourceRunOutcome {
        let articles_seen = u32::try_from(feed.articles.len()).unwrap_or(u32::MAX);
        let mut inserted = 0_u32;
        let mut rejected = 0_u32;

        for raw in feed.articles {
            let article = match normalize_article(source_id, raw, observed_at) {
                Ok(article) => article,
                Err(error) => {
                    rejected = rejected.saturating_add(1);
                    debug!(source_id = %source_id, reason = ?error, "news item rejected during normalization");
                    continue;
                }
            };
            let persisted = match upsert_article(&self.pool, &article).await {
                Ok(article) => article,
                Err(_) => {
                    let _ = finish_run(
                        &self.pool,
                        run_id,
                        SourceRunStatus::Failed,
                        observed_at,
                        articles_seen,
                        inserted,
                        Some("article_upsert_failed"),
                    )
                    .await;
                    return SourceRunOutcome {
                        source_id,
                        status: SourceRunStatus::Failed,
                        articles_seen,
                        articles_inserted: inserted,
                        articles_rejected: rejected,
                        attempts: 1,
                        terminal_code: Some("article_upsert_failed"),
                    };
                }
            };
            // `articles_inserted` counts successful DB-05 upserts, including
            // an existing row refreshed idempotently. An outbox failure must
            // not erase the fact that this article reached persistence.
            inserted = inserted.saturating_add(1);
            let event_key = article_event_key(&persisted);
            if self
                .outbox
                .enqueue_article_published(&persisted, &event_key)
                .await
                .is_err()
            {
                let _ = finish_run(
                    &self.pool,
                    run_id,
                    SourceRunStatus::Failed,
                    observed_at,
                    articles_seen,
                    inserted,
                    Some("outbox_enqueue_failed"),
                )
                .await;
                return SourceRunOutcome {
                    source_id,
                    status: SourceRunStatus::Failed,
                    articles_seen,
                    articles_inserted: inserted,
                    articles_rejected: rejected,
                    attempts: 1,
                    terminal_code: Some("outbox_enqueue_failed"),
                };
            }
            // `upsert_article` has committed before returning. Invalidate the
            // complete pagination family only after durable persistence and
            // the publication event succeed. Redis failure is logged but does
            // not turn accepted PostgreSQL state into a failed ingest.
            if self
                .cache_invalidator
                .invalidate_news_feed(source_id)
                .await
                .is_err()
            {
                warn!(
                    source_id = %source_id,
                    "news feed cache invalidation could not be emitted"
                );
            }
        }

        if finish_run(
            &self.pool,
            run_id,
            SourceRunStatus::Completed,
            observed_at,
            articles_seen,
            inserted,
            (rejected > 0).then_some("items_rejected"),
        )
        .await
        .is_err()
        {
            return SourceRunOutcome {
                source_id,
                status: SourceRunStatus::Failed,
                articles_seen,
                articles_inserted: inserted,
                articles_rejected: rejected,
                attempts: 1,
                terminal_code: Some("run_finalize_failed"),
            };
        }
        SourceRunOutcome {
            source_id,
            status: SourceRunStatus::Completed,
            articles_seen,
            articles_inserted: inserted,
            articles_rejected: rejected,
            attempts: 1,
            terminal_code: None,
        }
    }

    async fn alert_terminal(&self, source_id: Uuid, code: &str) {
        if self
            .outbox
            .enqueue_terminal_alert(source_id, bounded_code(code))
            .await
            .is_err()
        {
            warn!(source_id = %source_id, alert_code = bounded_code(code), "news terminal alert could not be enqueued");
        }
    }

    fn before_request(&self, source_id: Uuid) -> std::result::Result<bool, ()> {
        let mut providers = self.providers.lock().map_err(|_| ())?;
        let state = providers.entry(source_id).or_default();
        match state.health {
            CircuitHealth::CircuitOpen => {
                let opened_at = state.opened_at.ok_or(())?;
                if opened_at.elapsed() < Duration::from_secs(self.policy.open_for_seconds) {
                    return Err(());
                }
                if state.active_probes >= self.policy.probe_limit {
                    return Err(());
                }
                state.health = CircuitHealth::Probing;
                state.active_probes += 1;
                state.probe_successes = 0;
                Ok(true)
            }
            CircuitHealth::Probing => {
                if state.active_probes >= self.policy.probe_limit {
                    Err(())
                } else {
                    state.active_probes += 1;
                    Ok(true)
                }
            }
            CircuitHealth::Healthy | CircuitHealth::Degraded => Ok(false),
        }
    }

    fn record_success(&self, source_id: Uuid, probe: bool) {
        if let Ok(mut providers) = self.providers.lock() {
            let state = providers.entry(source_id).or_default();
            mark_provider_success(state, &self.policy, probe);
        }
    }

    fn record_failure(&self, source_id: Uuid, probe: bool) {
        if let Ok(mut providers) = self.providers.lock() {
            let state = providers.entry(source_id).or_default();
            mark_provider_failure(state, &self.policy, probe);
        }
    }

    async fn wait_for_rate_slot(&self, source_id: Uuid) {
        loop {
            let wait = {
                let Ok(mut providers) = self.providers.lock() else {
                    return;
                };
                let state = providers.entry(source_id).or_default();
                let now = Instant::now();
                let window = Duration::from_secs(u64::from(self.policy.window_seconds));
                let burst_window = Duration::from_secs(u64::from(
                    self.policy.window_seconds / self.policy.max_requests_per_window,
                ));
                while state
                    .request_times
                    .front()
                    .is_some_and(|started| now.duration_since(*started) >= window)
                {
                    state.request_times.pop_front();
                }
                while state
                    .burst_times
                    .front()
                    .is_some_and(|started| now.duration_since(*started) >= burst_window)
                {
                    state.burst_times.pop_front();
                }

                let max_requests =
                    usize::try_from(self.policy.max_requests_per_window).unwrap_or(usize::MAX);
                let burst_capacity =
                    usize::try_from(self.policy.burst_capacity).unwrap_or(usize::MAX);
                let mut waits = Vec::new();
                if state.request_times.len() >= max_requests {
                    if let Some(started) = state.request_times.front() {
                        waits.push(window.saturating_sub(now.duration_since(*started)));
                    }
                }
                if state.burst_times.len() >= burst_capacity {
                    if let Some(started) = state.burst_times.front() {
                        waits.push(burst_window.saturating_sub(now.duration_since(*started)));
                    }
                }

                if waits.is_empty() {
                    state.request_times.push_back(now);
                    state.burst_times.push_back(now);
                    None
                } else {
                    waits.into_iter().max()
                }
            };
            if let Some(wait) = wait {
                tokio::time::sleep(wait).await;
            } else {
                return;
            }
        }
    }
}

fn mark_provider_success(state: &mut ProviderState, policy: &IngestionPolicy, probe: bool) {
    state.consecutive_failures = 0;
    if probe {
        state.active_probes = state.active_probes.saturating_sub(1);
        state.probe_successes += 1;
        if state.probe_successes >= policy.successes_to_close {
            state.health = CircuitHealth::Healthy;
            state.opened_at = None;
            state.probe_successes = 0;
        }
    } else {
        state.health = CircuitHealth::Healthy;
    }
}

fn mark_provider_failure(state: &mut ProviderState, policy: &IngestionPolicy, probe: bool) {
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    if probe {
        state.active_probes = state.active_probes.saturating_sub(1);
        state.health = CircuitHealth::CircuitOpen;
        state.opened_at = Some(Instant::now());
        state.probe_successes = 0;
    } else if state.consecutive_failures >= policy.failure_threshold {
        state.health = CircuitHealth::CircuitOpen;
        state.opened_at = Some(Instant::now());
    } else {
        state.health = CircuitHealth::Degraded;
    }
}

fn failed_outcome(source_id: Uuid, code: &'static str) -> SourceRunOutcome {
    SourceRunOutcome {
        source_id,
        status: SourceRunStatus::Failed,
        articles_seen: 0,
        articles_inserted: 0,
        articles_rejected: 0,
        attempts: 0,
        terminal_code: Some(code),
    }
}

async fn start_run(pool: &PgPool, source_id: Uuid, started_at: DateTime<Utc>) -> Result<Uuid> {
    // TODO: replace with an orion-db repository/query call once DIV-08/DIV-12 lands.
    sqlx::query_scalar(
        "INSERT INTO news_ingestion_runs (source_id, started_at, status) \
         VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(source_id)
    .bind(started_at)
    .fetch_one(pool)
    .await
    .map_err(|_| anyhow!("news ingestion run could not be started"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReconciledRunCounts {
    error_count: u32,
}

fn reconcile_run_counts(articles_seen: u32, articles_inserted: u32) -> Result<ReconciledRunCounts> {
    if articles_inserted > articles_seen {
        return Err(anyhow!(
            "news ingestion run inserted count exceeds seen count"
        ));
    }
    Ok(ReconciledRunCounts {
        error_count: articles_seen - articles_inserted,
    })
}

async fn finish_run(
    pool: &PgPool,
    run_id: Uuid,
    status: SourceRunStatus,
    completed_at: DateTime<Utc>,
    articles_seen: u32,
    articles_inserted: u32,
    error_code: Option<&str>,
) -> Result<()> {
    // TODO: replace with an orion-db repository/query call once DIV-08/DIV-12 lands.
    // DB-05 intentionally stores `articles_seen` and `articles_inserted`, so
    // consumers derive the reconciled item-error count as seen - inserted.
    // `error_message` carries only the bounded run-level error code; no new
    // schema authority is introduced here.
    let counts = reconcile_run_counts(articles_seen, articles_inserted)?;
    let status = match status {
        SourceRunStatus::Completed => "completed",
        SourceRunStatus::Failed | SourceRunStatus::CircuitOpen => "failed",
    };
    let seen = i32::try_from(articles_seen).unwrap_or(i32::MAX);
    let inserted = i32::try_from(articles_inserted).unwrap_or(i32::MAX);
    sqlx::query(
        "UPDATE news_ingestion_runs SET completed_at = $2, status = $3, \
         articles_seen = $4, articles_inserted = $5, error_message = $6 WHERE id = $1",
    )
    .bind(run_id)
    .bind(completed_at)
    .bind(status)
    .bind(seen)
    .bind(inserted)
    .bind(error_code.map(bounded_code))
    .execute(pool)
    .await
    .map(|_| {
        debug!(
            run_id = %run_id,
            articles_seen,
            articles_inserted,
            articles_error_count = counts.error_count,
            "news ingestion run counts reconciled"
        );
    })
    .map_err(|_| {
        warn!(run_id = %run_id, run_status = status, "news ingestion run could not be finalized");
        anyhow!("news ingestion run could not be finalized")
    })
}

fn bounded_code(code: &str) -> &'static str {
    match code {
        "run_start_failed" => "run_start_failed",
        "source_not_admitted" => "source_not_admitted",
        "circuit_open" => "circuit_open",
        "provider_failure" => "provider_failure",
        "transport_failure" => "transport_failure",
        "source_url_missing" => "source_url_missing",
        "source_url_invalid" => "source_url_invalid",
        "provider_timeout" => "provider_timeout",
        "provider_rate_limited" => "provider_rate_limited",
        "provider_server_error" => "provider_server_error",
        "provider_unauthorized" => "provider_unauthorized",
        "provider_http_status" => "provider_http_status",
        "feed_body_too_large" => "feed_body_too_large",
        "unsafe_xml_declaration" => "unsafe_xml_declaration",
        "missing_feed_items" => "missing_feed_items",
        "too_many_feed_items" => "too_many_feed_items",
        "required_feed_field_missing" => "required_feed_field_missing",
        "article_upsert_failed" => "article_upsert_failed",
        "persistence_failed" => "persistence_failed",
        "outbox_enqueue_failed" => "outbox_enqueue_failed",
        "run_finalize_failed" => "run_finalize_failed",
        "items_rejected" => "items_rejected",
        _ if code.len() <= MAX_ERROR_CODE_BYTES
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') =>
        {
            "provider_failure"
        }
        _ => "provider_failure",
    }
}

fn parse_article_block(block: &str) -> std::result::Result<ProviderArticle, ProviderAdapterError> {
    let title = element_text(block, &["title"]);
    let summary = element_text(block, &["description", "summary"]);
    let content = element_text(block, &["content:encoded", "content"]);
    let url = element_text(block, &["link"]).or_else(|| element_attribute(block, "link", "href"));
    let external_id = element_text(block, &["guid", "id"]);
    let published_at = element_text(block, &["pubDate", "published", "updated"]);
    let image_url = element_attribute(block, "media:content", "url")
        .or_else(|| element_attribute(block, "media:thumbnail", "url"))
        .or_else(|| element_attribute(block, "enclosure", "url"));
    let author = element_text(block, &["dc:creator", "author"]);
    let category = element_text(block, &["category"]);
    let symbols = element_text(block, &["symbols"])
        .map(|value| value.split(',').map(str::to_owned).collect())
        .unwrap_or_default();

    if title.is_none()
        || url.is_none()
        || published_at.is_none()
        || (summary.is_none() && content.is_none())
    {
        return Err(ProviderAdapterError::new(
            ProviderFailureKind::InvalidPayload,
            "required_feed_field_missing",
        ));
    }
    Ok(ProviderArticle {
        external_id,
        title,
        summary,
        content,
        url,
        image_url,
        author,
        category,
        symbols,
        published_at,
    })
}

fn extract_blocks<'a>(body: &'a str, tag: &str) -> Option<Vec<&'a str>> {
    let lower = body.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut cursor = 0;
    let mut blocks = Vec::new();
    while let Some(relative) = lower[cursor..].find(&open) {
        let start = cursor + relative;
        let boundary = lower.as_bytes().get(start + open.len()).copied();
        if !matches!(
            boundary,
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
        ) {
            cursor = start + open.len();
            continue;
        }
        let open_end = lower[start..].find('>')? + start;
        if lower[open_end.saturating_sub(1)..=open_end].contains("/") {
            cursor = open_end + 1;
            continue;
        }
        let content_start = open_end + 1;
        let close_start = lower[content_start..].find(&close)? + content_start;
        blocks.push(&body[content_start..close_start]);
        cursor = close_start + close.len();
    }
    (!blocks.is_empty()).then_some(blocks)
}

fn element_text(block: &str, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        let (body, _) = find_element(block, name)?;
        Some(strip_markup_and_decode(body))
    })
}

fn element_attribute(block: &str, name: &str, attribute: &str) -> Option<String> {
    let lower = block.to_ascii_lowercase();
    let needle = format!("<{name}");
    let start = lower.find(&needle)?;
    let end = lower[start..].find('>')? + start;
    let opening = &block[start..=end];
    let opening_lower = opening.to_ascii_lowercase();
    let key = format!("{attribute}=");
    let key_start = opening_lower.find(&key)? + key.len();
    let quote = opening.as_bytes().get(key_start).copied()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let value_start = key_start + 1;
    let value_end = opening.as_bytes()[value_start..]
        .iter()
        .position(|byte| *byte == quote)?
        + value_start;
    Some(decode_xml_entities(&opening[value_start..value_end]))
}

fn find_element<'a>(block: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let lower = block.to_ascii_lowercase();
    let normalized_name = name.to_ascii_lowercase();
    let needle = format!("<{normalized_name}");
    let start = lower.find(&needle)?;
    let boundary = lower.as_bytes().get(start + needle.len()).copied();
    if !matches!(
        boundary,
        Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
    ) {
        return None;
    }
    let opening_end = lower[start..].find('>')? + start;
    let content_start = opening_end + 1;
    let closing = format!("</{normalized_name}>");
    let content_end = lower[content_start..].find(&closing)? + content_start;
    Some((
        &block[content_start..content_end],
        &block[start..=opening_end],
    ))
}

fn strip_markup_and_decode(value: &str) -> String {
    let value = value.replace("<![CDATA[", "").replace("]]>", "");
    let result = strip_markup(&remove_unsafe_blocks(&value));
    let decoded = decode_xml_entities(&result);
    let decoded = strip_markup(&remove_unsafe_blocks(&decoded));
    decode_xml_entities(&decoded)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .collect()
}

fn remove_unsafe_blocks(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    let lower = value.to_ascii_lowercase();
    loop {
        let next = ["script", "style", "iframe", "object"]
            .iter()
            .filter_map(|tag| {
                let relative_start = lower[cursor..].find(&format!("<{tag}"))?;
                let start = cursor + relative_start;
                let boundary = lower.as_bytes().get(start + tag.len() + 1).copied();
                matches!(
                    boundary,
                    Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
                )
                .then_some((start, *tag))
            })
            .min_by_key(|(start, _)| *start);
        let Some((start, tag)) = next else {
            result.push_str(&value[cursor..]);
            break;
        };
        result.push_str(&value[cursor..start]);
        let close = format!("</{tag}>");
        let Some(relative_end) = lower[start..].find(&close) else {
            return result;
        };
        cursor = start + relative_end + close.len();
    }
    result
}

fn strip_markup(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            character if !in_tag => result.push(character),
            _ => {}
        }
    }
    result
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn required_text(
    field: &'static str,
    value: Option<&str>,
) -> std::result::Result<String, NormalizationError> {
    optional_plain_text(field, value)?.ok_or(NormalizationError::MissingField(field))
}

fn optional_plain_text(
    field: &'static str,
    value: Option<&str>,
) -> std::result::Result<Option<String>, NormalizationError> {
    let Some(value) = value else { return Ok(None) };
    let value = strip_markup_and_decode(value);
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().any(char::is_control) {
        return Err(NormalizationError::UnsafeText(field));
    }
    if value.is_empty() {
        return Ok(None);
    }
    let max = match field {
        "title" => 500,
        "author" => 300,
        "category" => 100,
        "content" => 200_000,
        _ => 4_000,
    };
    if value.len() > max {
        return Err(NormalizationError::TooLong(field));
    }
    Ok(Some(value))
}

fn normalize_identifier(
    value: Option<&str>,
) -> std::result::Result<Option<String>, NormalizationError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 500
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(NormalizationError::InvalidIdentifier);
    }
    Ok(Some(value.to_owned()))
}

fn canonicalize_url(
    field: &'static str,
    value: Option<&str>,
) -> std::result::Result<String, NormalizationError> {
    let value = value.ok_or(NormalizationError::MissingField(field))?.trim();
    if value.is_empty()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(NormalizationError::InvalidUrl(field));
    }
    let lower = value.to_ascii_lowercase();
    if lower.contains("%00")
        || lower.contains("%0a")
        || lower.contains("%0d")
        || lower.contains("%09")
    {
        return Err(NormalizationError::InvalidUrl(field));
    }
    let without_fragment = value.split_once('#').map_or(value, |(prefix, _)| prefix);
    let (scheme, authority_and_path) = without_fragment
        .split_once("://")
        .ok_or(NormalizationError::InvalidUrl(field))?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(NormalizationError::InvalidUrl(field));
    }
    let (path_and_authority, query) = authority_and_path
        .split_once('?')
        .map_or((authority_and_path, None), |(prefix, query)| {
            (prefix, Some(query))
        });
    let authority = path_and_authority.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return Err(NormalizationError::InvalidUrl(field));
    }
    let suffix = &path_and_authority[authority.len()..];
    let suffix = if suffix == "/" { "" } else { suffix };
    let query = query.map(|query| {
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
    });
    let normalized = match query.as_deref() {
        Some(query) if !query.is_empty() => format!(
            "https://{}{}?{query}",
            authority.to_ascii_lowercase(),
            suffix
        ),
        _ => format!("https://{}{}", authority.to_ascii_lowercase(), suffix),
    };
    if normalized.len() > 4_000 {
        return Err(NormalizationError::TooLong(field));
    }
    Ok(normalized)
}

fn parse_utc_timestamp(
    value: Option<&str>,
) -> std::result::Result<DateTime<Utc>, NormalizationError> {
    let value = value
        .ok_or(NormalizationError::MissingField("published_at"))?
        .trim();
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .or_else(|_| {
            DateTime::parse_from_rfc2822(value).map(|timestamp| timestamp.with_timezone(&Utc))
        })
        .map_err(|_| NormalizationError::InvalidTimestamp)
}

fn normalize_symbols(symbols: Vec<String>) -> std::result::Result<Vec<String>, NormalizationError> {
    let mut normalized = HashSet::new();
    for symbol in symbols {
        let symbol = symbol.trim();
        if symbol.is_empty() {
            continue;
        }
        if symbol.len() > 32
            || !symbol.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '.' | '-' | '/' | ':' | '_' | '^')
            })
        {
            return Err(NormalizationError::InvalidSymbol);
        }
        normalized.insert(symbol.to_ascii_uppercase());
    }
    let mut normalized = normalized.into_iter().collect::<Vec<_>>();
    normalized.sort_unstable();
    Ok(normalized)
}

fn contains_ascii_case_insensitive(value: &str, needle: &str) -> bool {
    value.to_ascii_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-13T10:00:00Z")
            .expect("valid fixture timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn rss_adapter_parses_and_normalizes_html_rss() {
        let feed = r#"
            <rss><channel><item>
              <guid>wire-7</guid>
              <title>  Market &amp; Rates  </title>
              <description><![CDATA[<p>Safe summary</p><script>alert(1)</script>]]></description>
              <link>HTTPS://Example.com/story?utm_source=feed&amp;edition=global#fragment</link>
              <pubDate>Thu, 13 Aug 2026 09:00:00 GMT</pubDate>
            </item></channel></rss>
        "#;
        let raw = RssProviderAdapter::parse_feed(feed)
            .expect("valid RSS fixture")
            .articles
            .pop()
            .expect("one article");
        let article = normalize_article(Uuid::nil(), raw, observed_at()).expect("valid article");

        assert_eq!(article.title, "Market & Rates");
        assert_eq!(article.summary, "Safe summary");
        assert_eq!(article.content, "Safe summary");
        assert_eq!(article.url, "https://example.com/story?edition=global");
        assert_eq!(
            article.published_at.to_rfc3339(),
            "2026-08-13T09:00:00+00:00"
        );
    }

    #[test]
    fn atom_adapter_rejects_missing_required_fields() {
        let feed = r#"<feed><entry><title>Missing link and time</title><summary>body</summary></entry></feed>"#;
        let error = RssProviderAdapter::parse_feed(feed).expect_err("malformed entry must fail");
        assert_eq!(error.kind, ProviderFailureKind::InvalidPayload);
        assert_eq!(error.code, "required_feed_field_missing");
    }

    #[test]
    fn unsafe_url_and_control_text_are_not_normalized() {
        let raw = ProviderArticle {
            title: Some("headline\nwith whitespace".to_owned()),
            summary: Some("summary".to_owned()),
            url: Some("javascript:alert(1)".to_owned()),
            published_at: Some("2026-08-13T10:00:00Z".to_owned()),
            ..ProviderArticle::default()
        };
        assert!(matches!(
            normalize_article(Uuid::nil(), raw, observed_at()),
            Err(NormalizationError::InvalidUrl("url"))
        ));
    }

    #[test]
    fn validation_rejects_articles_that_bypass_normalization() {
        let article = normalize_article(
            Uuid::nil(),
            ProviderArticle {
                title: Some("headline".to_owned()),
                summary: Some("summary".to_owned()),
                url: Some("https://example.com/story".to_owned()),
                published_at: Some("2026-08-13T10:00:00Z".to_owned()),
                ..ProviderArticle::default()
            },
            observed_at(),
        )
        .expect("fixture article should normalize");
        let mut invalid = article;
        invalid.title.push(' ');

        assert_eq!(
            validate_article(&invalid),
            Err(NormalizationError::NotNormalized("title"))
        );
    }

    #[test]
    fn retry_policy_is_bounded_and_non_retryable_payloads_stop() {
        let policy = IngestionPolicy::default();
        assert_eq!(
            policy.retry_delay(1, ProviderFailureKind::Timeout),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            policy.retry_delay(2, ProviderFailureKind::RateLimited),
            Some(Duration::from_millis(500))
        );
        assert_eq!(policy.retry_delay(3, ProviderFailureKind::Timeout), None);
        assert_eq!(
            policy.retry_delay(1, ProviderFailureKind::InvalidPayload),
            None
        );
        assert_eq!(
            policy.retry_delay(1, ProviderFailureKind::Unauthorized),
            None
        );
    }

    #[test]
    fn replay_of_same_article_has_stable_event_identity() {
        let raw = ProviderArticle {
            external_id: Some("wire-7".to_owned()),
            title: Some("headline".to_owned()),
            summary: Some("summary".to_owned()),
            url: Some("https://example.com/story?utm_source=one#fragment".to_owned()),
            published_at: Some("2026-08-13T10:00:00Z".to_owned()),
            ..ProviderArticle::default()
        };
        let first = normalize_article(Uuid::nil(), raw.clone(), observed_at())
            .expect("first replay should normalize");
        let second = normalize_article(Uuid::nil(), raw, observed_at())
            .expect("second replay should normalize");

        assert_ne!(first.id, second.id);
        assert_eq!(first.url, second.url);
        assert_eq!(article_event_key(&first), article_event_key(&second));
    }

    #[test]
    fn partial_provider_failure_does_not_open_another_source() {
        let policy = IngestionPolicy::default();
        let first_source = Uuid::from_u128(1);
        let second_source = Uuid::from_u128(2);
        let mut providers = HashMap::new();

        for _ in 0..policy.failure_threshold {
            let state = providers.entry(first_source).or_default();
            mark_provider_failure(state, &policy, false);
        }

        assert_eq!(
            providers.get(&first_source).map(|state| state.health),
            Some(CircuitHealth::CircuitOpen)
        );
        providers.entry(second_source).or_default();
        assert_eq!(
            providers.get(&second_source).map(|state| state.health),
            Some(CircuitHealth::Healthy)
        );
    }

    #[test]
    fn ingestion_run_counts_reconcile_seen_inserted_and_errors() {
        let completed = SourceRunOutcome {
            source_id: Uuid::nil(),
            status: SourceRunStatus::Completed,
            articles_seen: 5,
            articles_inserted: 3,
            articles_rejected: 2,
            attempts: 1,
            terminal_code: None,
        };
        assert_eq!(completed.error_count(), 2);
        assert!(completed.counts_reconcile());

        let partial_failure = SourceRunOutcome {
            articles_seen: 5,
            articles_inserted: 2,
            articles_rejected: 1,
            status: SourceRunStatus::Failed,
            ..completed.clone()
        };
        assert_eq!(partial_failure.error_count(), 3);
        assert!(partial_failure.counts_reconcile());

        let invalid = SourceRunOutcome {
            articles_seen: 2,
            articles_inserted: 3,
            articles_rejected: 0,
            ..completed
        };
        assert_eq!(invalid.error_count(), 0);
        assert!(!invalid.counts_reconcile());
        assert!(reconcile_run_counts(2, 3).is_err());
        assert_eq!(reconcile_run_counts(5, 3).unwrap().error_count, 2);
    }
}
