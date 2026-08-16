use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::FromRow;
use uuid::Uuid;

/// Final actual-value facts handed off by the approved provider ingestion
/// boundary. The worker validates these facts again before settlement.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct AdvancedActualRecord {
    pub question_id: Uuid,
    pub value: Decimal,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_id: String,
    pub source_version: String,
    pub is_final: bool,
}
