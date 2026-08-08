use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// A configured upstream feed for market news.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NewsSource {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub external_id: Option<String>,
    pub source_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A normalized article stored from a market-news provider.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
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

/// One attempt to fetch and normalize articles from a source.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NewsIngestionRun {
    pub id: Uuid,
    pub source_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: String,
    pub articles_seen: i32,
    pub articles_inserted: i32,
    pub error_message: Option<String>,
}
