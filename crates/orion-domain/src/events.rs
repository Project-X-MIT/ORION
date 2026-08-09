use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    ContractError, EventId, NotificationId, Rating, RatingEntryId, UserId, VersionedEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventContractSpec {
    pub event_type: &'static str,
    pub owner: &'static str,
    pub current_version: u16,
    pub minimum_supported_version: u16,
}

pub const EVENT_CONTRACTS: &[EventContractSpec] = &[
    EventContractSpec {
        event_type: RatingUpdatedV1::EVENT_TYPE,
        owner: "akaidk",
        current_version: RatingUpdatedV1::SCHEMA_VERSION,
        minimum_supported_version: 1,
    },
    EventContractSpec {
        event_type: NotificationRequestedV1::EVENT_TYPE,
        owner: "divi912",
        current_version: NotificationRequestedV1::SCHEMA_VERSION,
        minimum_supported_version: 1,
    },
];

#[must_use]
pub fn event_contract(event_type: &str) -> Option<&'static EventContractSpec> {
    EVENT_CONTRACTS
        .iter()
        .find(|entry| entry.event_type == event_type)
}

pub fn ensure_event_compatible(event_type: &str, version: u16) -> Result<(), ContractError> {
    let compatible = event_contract(event_type).is_some_and(|contract| {
        version >= contract.minimum_supported_version && version <= contract.current_version
    });
    if compatible {
        return Ok(());
    }

    Err(ContractError::UnsupportedEventVersion {
        event_type: event_type.to_owned(),
        version,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub event_id: EventId,
    pub event_type: String,
    pub schema_version: u16,
    pub occurred_at: DateTime<Utc>,
    pub producer: String,
    pub payload: T,
}

impl<T: VersionedEvent> EventEnvelope<T> {
    #[must_use]
    pub fn new(
        event_id: EventId,
        occurred_at: DateTime<Utc>,
        producer: impl Into<String>,
        payload: T,
    ) -> Self {
        Self {
            event_id,
            event_type: T::EVENT_TYPE.to_owned(),
            schema_version: T::SCHEMA_VERSION,
            occurred_at,
            producer: producer.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RatingReason {
    BasicQuiz,
    AdvancedQuiz,
    ResearchAward,
    AdministrativeCorrection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingUpdatedV1 {
    pub rating_entry_id: RatingEntryId,
    pub user_id: UserId,
    pub previous_rating: Rating,
    pub current_rating: Rating,
    pub delta: i32,
    pub reason: RatingReason,
    pub source_id: Uuid,
}

impl VersionedEvent for RatingUpdatedV1 {
    const EVENT_TYPE: &'static str = "orion.rating.updated";
    const SCHEMA_VERSION: u16 = 1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    RatingChanged,
    ResearchDecision,
    LearningProgress,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationRequestedV1 {
    pub notification_id: NotificationId,
    pub recipient_id: UserId,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub action_url: Option<String>,
    pub deduplication_key: String,
}

impl VersionedEvent for NotificationRequestedV1 {
    const EVENT_TYPE: &'static str = "orion.notification.requested";
    const SCHEMA_VERSION: u16 = 1;
}
