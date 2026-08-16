use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub schema_version: i32,
    pub request_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub retry_count: i32,
    pub job_status: String,
    pub job_attempts: i32,
    pub job_error: Option<String>,
    pub job_next_retry_at: Option<DateTime<Utc>>,
    pub job_started_at: Option<DateTime<Utc>>,
    pub lease_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxJobStatus {
    Queued,
    Running,
    Completed,
    Retry,
    DeadLetter,
}
