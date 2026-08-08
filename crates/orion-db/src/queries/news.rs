use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::NewsArticle;

const LATEST_ARTICLES: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    ORDER BY published_at DESC, id DESC
    LIMIT $1 OFFSET $2
"#;

const ARTICLE_BY_ID: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    WHERE id = $1
"#;

const ARTICLES_BY_SOURCE_ID: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    WHERE source_id = $1
    ORDER BY published_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

const ARTICLES_BY_CATEGORY: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    WHERE category = $1
    ORDER BY published_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

const ARTICLES_BY_SYMBOL: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    WHERE $1 = ANY(symbols)
    ORDER BY published_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

const SEARCH_ARTICLES: &str = r#"
    SELECT id, source_id, external_id, title, summary, content, url,
           image_url, author, category, symbols, published_at, ingested_at,
           created_at, updated_at
    FROM news_articles
    WHERE to_tsvector(
              'simple',
              concat_ws(' ', title, summary, content, author, category)
          ) @@ plainto_tsquery('simple', $1)
    ORDER BY published_at DESC, id DESC
    LIMIT $2 OFFSET $3
"#;

const UPSERT_ARTICLE: &str = r#"
    WITH advisory_locks AS (
        SELECT
            pg_advisory_xact_lock(
                hashtextextended('news:url:' || $7, 0)
            ),
            pg_advisory_xact_lock(
                hashtextextended(
                    'news:external:' || $2::text || ':' || COALESCE($3, ''),
                    0
                )
            )
    ),
    existing_article AS (
        SELECT article.id
        FROM news_articles AS article
        CROSS JOIN advisory_locks
        WHERE article.url = $7
           OR (
               $3 IS NOT NULL
               AND article.source_id = $2
               AND article.external_id = $3
           )
        ORDER BY CASE WHEN article.url = $7 THEN 0 ELSE 1 END, article.id
        LIMIT 1
    )
    INSERT INTO news_articles (
        id, source_id, external_id, title, summary, content, url, image_url,
        author, category, symbols, published_at, ingested_at
    )
    SELECT
        COALESCE((SELECT id FROM existing_article), $1),
        $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
    ON CONFLICT (id) DO UPDATE SET
        source_id = EXCLUDED.source_id,
        external_id = EXCLUDED.external_id,
        title = EXCLUDED.title,
        summary = EXCLUDED.summary,
        content = EXCLUDED.content,
        image_url = EXCLUDED.image_url,
        author = EXCLUDED.author,
        category = EXCLUDED.category,
        symbols = EXCLUDED.symbols,
        published_at = EXCLUDED.published_at,
        ingested_at = EXCLUDED.ingested_at,
        updated_at = CURRENT_TIMESTAMP
    RETURNING id, source_id, external_id, title, summary, content, url,
              image_url, author, category, symbols, published_at, ingested_at,
              created_at, updated_at
"#;

/// Returns newest articles, using a stable ID tie-breaker for pagination.
pub async fn latest_news(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(LATEST_ARTICLES)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Backwards-compatible shorthand for [`latest_news`].
pub async fn latest(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<NewsArticle>> {
    latest_news(pool, limit, offset).await
}

/// Returns one page of the public news feed, newest articles first.
pub async fn paginated_feed(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<NewsArticle>> {
    latest_news(pool, limit, offset).await
}

pub async fn find_by_id(pool: &PgPool, article_id: Uuid) -> Result<Option<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(ARTICLE_BY_ID)
        .bind(article_id)
        .fetch_optional(pool)
        .await
}

pub async fn by_source_id(
    pool: &PgPool,
    source_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(ARTICLES_BY_SOURCE_ID)
        .bind(source_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn by_category(
    pool: &PgPool,
    category: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(ARTICLES_BY_CATEGORY)
        .bind(category)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn by_symbol(
    pool: &PgPool,
    symbol: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(ARTICLES_BY_SYMBOL)
        .bind(symbol)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn search(
    pool: &PgPool,
    search_text: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<NewsArticle>> {
    sqlx::query_as::<_, NewsArticle>(SEARCH_ARTICLES)
        .bind(search_text)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Inserts an article or refreshes the existing article with the same URL.
pub async fn upsert_article(pool: &PgPool, article: &NewsArticle) -> Result<NewsArticle> {
    sqlx::query_as::<_, NewsArticle>(UPSERT_ARTICLE)
        .bind(article.id)
        .bind(article.source_id)
        .bind(&article.external_id)
        .bind(&article.title)
        .bind(&article.summary)
        .bind(&article.content)
        .bind(&article.url)
        .bind(&article.image_url)
        .bind(&article.author)
        .bind(&article.category)
        .bind(&article.symbols)
        .bind(article.published_at)
        .bind(article.ingested_at)
        .fetch_one(pool)
        .await
}

/// Backwards-compatible shorthand for [`upsert_article`].
pub async fn upsert(pool: &PgPool, article: &NewsArticle) -> Result<NewsArticle> {
    upsert_article(pool, article).await
}
