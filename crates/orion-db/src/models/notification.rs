use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub action_url: Option<String>,
    pub deduplication_key: String,
    pub read_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Notification {
    #[must_use]
    pub const fn is_read(&self) -> bool {
        self.read_at.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewNotification<'a> {
    pub user_id: Uuid,
    pub kind: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub action_url: Option<&'a str>,
    pub deduplication_key: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}
