use std::env;

use chrono::{DateTime, Utc};
use orion_db::{
    models::NewsArticle,
    pool,
    queries::news::{latest_feed_filtered, upsert_article, NewsFeedFilters},
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct TestDatabase {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl TestDatabase {
    async fn create() -> Option<Self> {
        // TODO(CI): configure ORION_TEST_DATABASE_URL (or DATABASE_URL) so
        // PostgreSQL integration tests execute instead of being skipped.
        let database_url = env::var("ORION_TEST_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .ok()?;
        let admin = match pool::connect(&database_url).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Skipping PostgreSQL news idempotency test: {error}");
                return None;
            }
        };
        let schema = format!("orion_news_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated news test schema");

        create_schema_objects(&admin, &schema).await;

        let search_path = format!("SET search_path TO {schema}, public");
        let connection_search_path = search_path.clone();
        let test_pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _metadata| {
                let search_path = connection_search_path.clone();
                Box::pin(async move {
                    sqlx::query(&search_path)
                        .execute(connection)
                        .await
                        .map(|_| ())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect isolated news test pool");

        Some(Self {
            pool: test_pool,
            admin,
            schema,
        })
    }

    async fn cleanup(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .expect("drop isolated news test schema");
        self.admin.close().await;
    }
}

async fn create_schema_objects(admin: &PgPool, schema: &str) {
    let statements = [
        format!(
            "CREATE TABLE {schema}.news_sources (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                external_id TEXT UNIQUE,
                source_url TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        ),
        format!(
            "CREATE TABLE {schema}.news_articles (
                id UUID PRIMARY KEY,
                source_id UUID NOT NULL REFERENCES {schema}.news_sources (id) ON DELETE RESTRICT,
                external_id TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                content TEXT NOT NULL,
                url TEXT NOT NULL UNIQUE,
                image_url TEXT,
                author TEXT,
                category TEXT,
                symbols TEXT[] NOT NULL DEFAULT '{{}}',
                published_at TIMESTAMPTZ NOT NULL,
                ingested_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"
        ),
        format!(
            "CREATE UNIQUE INDEX news_articles_source_external_id_idx
             ON {schema}.news_articles (source_id, external_id)
             WHERE external_id IS NOT NULL"
        ),
    ];

    for statement in statements {
        sqlx::query(&statement)
            .execute(admin)
            .await
            .expect("create isolated news test table");
    }
}

fn article(id: Uuid, source_id: Uuid, observed_at: DateTime<Utc>) -> NewsArticle {
    NewsArticle {
        id,
        source_id,
        external_id: Some("provider-article-7".to_owned()),
        title: "Market headline".to_owned(),
        summary: "Market summary".to_owned(),
        content: "Market content".to_owned(),
        url: "https://example.com/markets/story-7".to_owned(),
        image_url: None,
        author: Some("Wire".to_owned()),
        category: Some("markets".to_owned()),
        symbols: vec!["ORION".to_owned()],
        published_at: observed_at,
        ingested_at: observed_at,
        created_at: observed_at,
        updated_at: observed_at,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replaying_same_window_is_idempotent_even_concurrently() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };

    let source_id = Uuid::new_v4();
    let observed_at = DateTime::parse_from_rfc3339("2026-08-13T10:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc);
    sqlx::query(
        "INSERT INTO news_sources (id, name, slug, external_id, source_url)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(source_id)
    .bind("Test Wire")
    .bind("test-wire")
    .bind("test-wire")
    .bind("https://example.com/feed")
    .execute(&database.pool)
    .await
    .expect("insert test news source");

    let first = upsert_article(
        &database.pool,
        &article(Uuid::new_v4(), source_id, observed_at),
    )
    .await
    .expect("first article upsert");
    let replay = upsert_article(
        &database.pool,
        &article(Uuid::new_v4(), source_id, observed_at),
    )
    .await
    .expect("replayed article upsert");
    assert_eq!(first.id, replay.id, "sequential replay must reuse row ID");

    let concurrent_article_one = article(Uuid::new_v4(), source_id, observed_at);
    let concurrent_article_two = article(Uuid::new_v4(), source_id, observed_at);
    let concurrent_one = upsert_article(&database.pool, &concurrent_article_one);
    let concurrent_two = upsert_article(&database.pool, &concurrent_article_two);
    let (concurrent_one, concurrent_two) = tokio::join!(concurrent_one, concurrent_two);
    let concurrent_one = concurrent_one.expect("first concurrent replay upsert");
    let concurrent_two = concurrent_two.expect("second concurrent replay upsert");
    assert_eq!(concurrent_one.id, first.id);
    assert_eq!(concurrent_two.id, first.id);

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM news_articles")
        .fetch_one(&database.pool)
        .await
        .expect("count replayed articles");
    assert_eq!(
        row_count, 1,
        "replaying a window must not create duplicates"
    );

    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cursor_pagination_has_no_gaps_or_duplicates_for_stable_data() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };

    let source_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO news_sources (id, name, slug, external_id, source_url)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(source_id)
    .bind("Pagination Test Wire")
    .bind("pagination-test-wire")
    .bind("pagination-test-wire")
    .bind("https://example.com/pagination-feed")
    .execute(&database.pool)
    .await
    .expect("insert pagination test news source");

    let first_timestamp = DateTime::parse_from_rfc3339("2026-08-13T10:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc);
    let second_timestamp = DateTime::parse_from_rfc3339("2026-08-13T11:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc);
    let third_timestamp = DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
        .expect("valid fixture timestamp")
        .with_timezone(&Utc);
    let fixtures = [
        (Uuid::from_u128(5), third_timestamp),
        (Uuid::from_u128(3), third_timestamp),
        (Uuid::from_u128(4), second_timestamp),
        (Uuid::from_u128(2), first_timestamp),
        (Uuid::from_u128(1), first_timestamp),
    ];

    for (id, published_at) in fixtures {
        sqlx::query(
            "INSERT INTO news_articles
                (id, source_id, external_id, title, summary, content, url,
                 category, symbols, published_at, ingested_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10, $10, $10)",
        )
        .bind(id)
        .bind(source_id)
        .bind(format!("pagination-{id}"))
        .bind(format!("Pagination article {id}"))
        .bind("Pagination summary")
        .bind("Pagination content")
        .bind(format!("https://example.com/pagination/{id}"))
        .bind("markets")
        .bind(Vec::<String>::new())
        .bind(published_at)
        .execute(&database.pool)
        .await
        .expect("insert pagination test article");
    }

    let expected_ids = fixtures.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
    let mut cursor = NewsFeedFilters::default();
    let mut seen_ids = Vec::new();

    loop {
        let page = latest_feed_filtered(&database.pool, 2, 0, cursor)
            .await
            .expect("fetch cursor page");
        if page.is_empty() {
            break;
        }

        assert!(page.len() <= 2, "page exceeds requested limit");
        seen_ids.extend(page.iter().map(|article| article.id));
        let last = page.last().expect("non-empty page has a last article");
        cursor = NewsFeedFilters {
            cursor_published_at: Some(last.published_at),
            cursor_id: Some(last.id),
            ..NewsFeedFilters::default()
        };
    }

    assert_eq!(
        seen_ids, expected_ids,
        "cursor pages must cover each stable article exactly once in feed order"
    );

    database.cleanup().await;
}
