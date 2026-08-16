use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use orion_common::{ErrorCode, PageRequest, MAX_PAGE_SIZE};
use orion_db::{
    error::DatabaseError,
    models::NewsFeedArticle,
    queries::news::{latest_feed_filtered, NewsFeedFilters},
};
use orion_redis::cache::news as news_cache;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::{request_id, state::AppState, ApiProblem};

const PUBLIC_CACHE_CONTROL: &str = "no-store";

/// Public news-feed routes. Div must mount this router at `/api/v1/news`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_feed))
        .route("/latest", get(latest_feed_page))
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PaginationQuery {
    limit: Option<u32>,
    offset: Option<u64>,
    cursor: Option<String>,
    category: Option<String>,
    symbol: Option<String>,
    source_id: Option<String>,
}

/// Only fields that are safe for public feed consumers are serialized. DB
/// ingestion IDs and operational timestamps never reach the UI or cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsArticleResponse {
    pub id: uuid::Uuid,
    pub source_id: uuid::Uuid,
    pub source_name: String,
    pub source_slug: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub url: String,
    pub image_url: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub symbols: Vec<String>,
    pub published_at: DateTime<Utc>,
}

impl From<NewsFeedArticle> for NewsArticleResponse {
    fn from(article: NewsFeedArticle) -> Self {
        Self {
            id: article.id,
            source_id: article.source_id,
            source_name: article.source_name,
            source_slug: article.source_slug,
            title: article.title,
            summary: article.summary,
            content: article.content,
            // The registered source authority is the approved-host policy.
            // Invalid or unapproved article URLs become an unavailable link.
            url: approved_outbound_url(&article.url, article.source_url.as_deref())
                .unwrap_or_default(),
            image_url: article.image_url,
            author: article.author,
            category: article.category,
            symbols: article.symbols,
            published_at: article.published_at,
        }
    }
}

/// Allows outbound links only when the normalized article URL is HTTPS and
/// matches the authority (host and port) registered for its source. The
/// frontend repeats scheme/credential checks as defense in depth.
fn approved_outbound_url(article_url: &str, source_url: Option<&str>) -> Option<String> {
    let article_uri = article_url.parse::<Uri>().ok()?;
    let source_uri = source_url?.parse::<Uri>().ok()?;
    let article_scheme = article_uri.scheme()?.as_str();
    let source_scheme = source_uri.scheme()?.as_str();
    if !article_scheme.eq_ignore_ascii_case("https") || !source_scheme.eq_ignore_ascii_case("https")
    {
        return None;
    }

    let article_authority = article_uri.authority()?;
    let source_authority = source_uri.authority()?;
    if article_authority.as_str().contains('@')
        || source_authority.as_str().contains('@')
        || !article_authority
            .as_str()
            .eq_ignore_ascii_case(source_authority.as_str())
    {
        return None;
    }

    Some(article_url.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsFeedResponse {
    pub items: Vec<NewsArticleResponse>,
    pub limit: u32,
    pub offset: u64,
    pub has_more: bool,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FeedCursor {
    published_at: DateTime<Utc>,
    id: Uuid,
}

async fn latest_feed_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaginationQuery>,
) -> Result<Response, ApiProblem> {
    list_feed(
        State(state),
        headers,
        Query(PaginationQuery {
            limit: query.limit,
            offset: Some(0),
            cursor: None,
            category: query.category,
            symbol: query.symbol,
            source_id: query.source_id,
        }),
    )
    .await
}

async fn list_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaginationQuery>,
) -> Result<Response, ApiProblem> {
    let request_id = request_id(&headers);
    let page = pagination(query, request_id)?;

    let cacheable = page.cursor.is_none()
        && page.category.is_none()
        && page.symbol.is_none()
        && page.source_id.is_none();
    // TODO(Div): register a versioned cache key containing cursor and approved
    // filter values; until then those request shapes intentionally bypass Redis.
    if cacheable {
        if let Some(cached) = cached_feed_or_database(
            news_cache::get::<NewsFeedResponse>(&state.redis, page.limit, page.offset_u64).await,
            page.limit,
            page.offset_u64,
        ) {
            return Ok(public_success(&headers, cached));
        }
    }

    let mut articles = latest_feed_filtered(
        &state.db,
        i64::from(page.limit) + 1,
        page.offset,
        NewsFeedFilters {
            category: page.category.as_deref(),
            symbol: page.symbol.as_deref(),
            source_id: page.source_id,
            cursor_published_at: page.cursor.map(|cursor| cursor.published_at),
            cursor_id: page.cursor.map(|cursor| cursor.id),
        },
    )
    .await
    .map_err(|error| database_problem(error, request_id))?;
    let has_more = articles.len() > usize::try_from(page.limit).expect("validated page limit fits");
    if has_more {
        articles.pop();
    }

    let mut response = NewsFeedResponse {
        items: articles
            .into_iter()
            .map(NewsArticleResponse::from)
            .collect(),
        limit: page.limit,
        offset: page.offset_u64,
        has_more,
        next_cursor: None,
    };
    if has_more {
        response.next_cursor = response.items.last().map(encode_cursor);
    }

    if cacheable {
        if let Err(error) =
            news_cache::set(&state.redis, page.limit, page.offset_u64, &response).await
        {
            warn!(
                error = %error,
                limit = page.limit,
                offset = page.offset_u64,
                "news feed cache fill failed"
            );
        }
    }

    Ok(public_success(&headers, response))
}

/// Turns Redis failure into a cache miss. The caller continues to the
/// PostgreSQL query, which remains authoritative for the feed response.
fn cached_feed_or_database<T>(
    result: Result<Option<news_cache::NewsFeedCache<T>>, news_cache::NewsCacheError>,
    limit: u32,
    offset: u64,
) -> Option<T> {
    match result {
        Ok(Some(cached)) => Some(cached.value),
        Ok(None) => None,
        Err(error) => {
            warn!(
                error = %error,
                limit,
                offset,
                "news feed cache read failed; using PostgreSQL"
            );
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedPage {
    limit: u32,
    offset: i64,
    offset_u64: u64,
    cursor: Option<FeedCursor>,
    category: Option<String>,
    symbol: Option<String>,
    source_id: Option<Uuid>,
}

fn pagination(
    query: PaginationQuery,
    request_id: orion_common::RequestId,
) -> Result<ValidatedPage, ApiProblem> {
    let limit = query.limit.unwrap_or(PageRequest::DEFAULT.limit);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(validation(request_id, "limit must be between 1 and 100"));
    }
    let offset_u64 = query.offset.unwrap_or(0);
    if query.cursor.is_some() && offset_u64 != 0 {
        return Err(validation(
            request_id,
            "cursor and offset cannot be used together",
        ));
    }
    let offset =
        i64::try_from(offset_u64).map_err(|_| validation(request_id, "offset is too large"))?;
    Ok(ValidatedPage {
        limit,
        offset,
        offset_u64,
        cursor: query
            .cursor
            .as_deref()
            .map(|value| decode_cursor(value, request_id))
            .transpose()?,
        category: query
            .category
            .as_deref()
            .map(|value| normalize_category(value, request_id))
            .transpose()?,
        symbol: query
            .symbol
            .as_deref()
            .map(|value| normalize_symbol(value, request_id))
            .transpose()?,
        source_id: query
            .source_id
            .as_deref()
            .map(|value| {
                Uuid::parse_str(value.trim())
                    .map_err(|_| validation(request_id, "source_id must be a valid UUID"))
            })
            .transpose()?,
    })
}

fn normalize_category(
    value: &str,
    request_id: orion_common::RequestId,
) -> Result<String, ApiProblem> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 100
        || value.chars().any(|character| character.is_control())
        || value
            .chars()
            .any(|character| matches!(character, '<' | '>'))
    {
        return Err(validation(request_id, "category is invalid"));
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_symbol(
    value: &str,
    request_id: orion_common::RequestId,
) -> Result<String, ApiProblem> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 32
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '-' | '/' | ':' | '_' | '^')
        })
    {
        return Err(validation(request_id, "symbol is invalid"));
    }
    Ok(value.to_ascii_uppercase())
}

fn encode_cursor(article: &NewsArticleResponse) -> String {
    let cursor = FeedCursor {
        published_at: article.published_at,
        id: article.id,
    };
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).expect("feed cursor serializes"))
}

fn decode_cursor(
    value: &str,
    request_id: orion_common::RequestId,
) -> Result<FeedCursor, ApiProblem> {
    if value.is_empty() || value.len() > 256 {
        return Err(validation(request_id, "cursor is invalid"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| validation(request_id, "cursor is invalid"))?;
    let cursor = serde_json::from_slice::<FeedCursor>(&bytes)
        .map_err(|_| validation(request_id, "cursor is invalid"))?;
    if cursor.id.is_nil() {
        return Err(validation(request_id, "cursor is invalid"));
    }
    Ok(cursor)
}

fn database_problem(error: sqlx::Error, request_id: orion_common::RequestId) -> ApiProblem {
    ApiProblem::from(DatabaseError::from_sqlx(error)).with_request_id(request_id)
}

fn validation(request_id: orion_common::RequestId, message: &'static str) -> ApiProblem {
    ApiProblem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ValidationFailed,
        message,
    )
    .with_request_id(request_id)
}

fn public_success<T: Serialize>(headers: &HeaderMap, data: T) -> Response {
    let mut response = crate::success(headers, data).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PUBLIC_CACHE_CONTROL),
    );
    response
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use fred::error::{Error as FredError, ErrorKind};
    use orion_redis::RedisClientError;
    use serde_json::json;

    use super::{
        cached_feed_or_database, decode_cursor, encode_cursor, pagination, NewsArticleResponse,
        PaginationQuery,
    };
    use orion_common::RequestId;

    #[test]
    fn pagination_has_bounded_defaults_and_offset() {
        let request_id = RequestId::from_uuid(uuid::Uuid::from_u128(1));
        let page = pagination(PaginationQuery::default(), request_id).unwrap();
        assert_eq!(page.limit, 20);
        assert_eq!(page.offset, 0);
        assert_eq!(page.offset_u64, 0);

        assert!(pagination(
            PaginationQuery {
                limit: Some(101),
                offset: None,
                ..PaginationQuery::default()
            },
            request_id
        )
        .is_err());
    }

    #[test]
    fn public_article_projection_excludes_internal_fields() {
        let article = NewsArticleResponse {
            id: uuid::Uuid::from_u128(1),
            source_id: uuid::Uuid::from_u128(2),
            source_name: "SEC".to_owned(),
            source_slug: "sec".to_owned(),
            title: "Headline".to_owned(),
            summary: "Summary".to_owned(),
            content: "Content".to_owned(),
            url: "https://example.com/story".to_owned(),
            image_url: None,
            author: None,
            category: Some("markets".to_owned()),
            symbols: vec!["ORION".to_owned()],
            published_at: chrono::DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        };
        let value = serde_json::to_value(article).unwrap();
        assert_eq!(value["source_name"], json!("SEC"));
        assert!(value.get("external_id").is_none());
        assert!(value.get("ingested_at").is_none());
        assert!(value.get("updated_at").is_none());
    }

    #[test]
    fn outbound_url_requires_the_registered_source_authority() {
        assert_eq!(
            super::approved_outbound_url(
                "https://www.sec.gov/news/story",
                Some("https://www.sec.gov/news/pressreleases.rss"),
            ),
            Some("https://www.sec.gov/news/story".to_owned())
        );
        assert!(super::approved_outbound_url(
            "https://evil.example/story",
            Some("https://www.sec.gov/news/pressreleases.rss"),
        )
        .is_none());
        assert!(super::approved_outbound_url("https://www.sec.gov/news/story", None,).is_none());
        assert!(super::approved_outbound_url(
            "https://user:password@www.sec.gov/news/story",
            Some("https://www.sec.gov/news/pressreleases.rss"),
        )
        .is_none());
    }

    #[test]
    fn cursor_round_trips_the_published_at_and_id_tie_breaker() {
        let article = NewsArticleResponse {
            id: uuid::Uuid::from_u128(42),
            source_id: uuid::Uuid::from_u128(2),
            source_name: "SEC".to_owned(),
            source_slug: "sec".to_owned(),
            title: "Headline".to_owned(),
            summary: "Summary".to_owned(),
            content: "Content".to_owned(),
            url: "https://example.com/story".to_owned(),
            image_url: None,
            author: None,
            category: None,
            symbols: Vec::new(),
            published_at: chrono::DateTime::parse_from_rfc3339("2026-08-14T10:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        };
        let request_id = RequestId::from_uuid(uuid::Uuid::from_u128(1));
        let cursor = encode_cursor(&article);
        let decoded = decode_cursor(&cursor, request_id).unwrap();
        assert_eq!(decoded.id, article.id);
        assert_eq!(decoded.published_at, article.published_at);
    }

    #[test]
    fn cursor_and_offset_cannot_be_ambiguous() {
        let request_id = RequestId::from_uuid(uuid::Uuid::from_u128(1));
        assert!(pagination(
            PaginationQuery {
                offset: Some(1),
                cursor: Some("bad".to_owned()),
                ..PaginationQuery::default()
            },
            request_id
        )
        .is_err());
    }

    #[test]
    fn approved_filters_are_normalized_before_querying() {
        let request_id = RequestId::from_uuid(uuid::Uuid::from_u128(1));
        let page = pagination(
            PaginationQuery {
                category: Some("  Global Markets  ".to_owned()),
                symbol: Some("aapl".to_owned()),
                ..PaginationQuery::default()
            },
            request_id,
        )
        .unwrap();

        assert_eq!(page.category.as_deref(), Some("global markets"));
        assert_eq!(page.symbol.as_deref(), Some("AAPL"));
    }

    #[test]
    fn malformed_filter_values_are_rejected() {
        let request_id = RequestId::from_uuid(uuid::Uuid::from_u128(1));
        assert!(pagination(
            PaginationQuery {
                category: Some("<script>".to_owned()),
                ..PaginationQuery::default()
            },
            request_id,
        )
        .is_err());
        assert!(pagination(
            PaginationQuery {
                symbol: Some("AAPL<script>".to_owned()),
                ..PaginationQuery::default()
            },
            request_id,
        )
        .is_err());
    }

    #[test]
    fn redis_outage_is_a_non_fatal_cache_miss_for_database_fallback() {
        let redis_error =
            orion_redis::cache::news::NewsCacheError::Redis(RedisClientError::Connection(
                FredError::new(ErrorKind::Timeout, "Redis unavailable in fallback fixture"),
            ));
        let result = cached_feed_or_database::<NewsArticleResponse>(Err(redis_error), 20, 0);

        assert!(result.is_none());

        let cached = orion_redis::cache::news::NewsFeedCache {
            schema_version: orion_redis::cache::news::NEWS_FEED_CACHE_SCHEMA_VERSION,
            cached_at: Utc::now(),
            value: "cached",
        };
        assert_eq!(
            cached_feed_or_database(Ok(Some(cached)), 20, 0),
            Some("cached")
        );
    }
}
