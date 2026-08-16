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
    EventContractSpec {
        event_type: "orion.leaderboard.snapshot.completed",
        owner: "ShauryaBijalwan",
        current_version: 1,
        minimum_supported_version: 1,
    },
    EventContractSpec {
        event_type: AdvancedSubmissionRequestedV1::EVENT_TYPE,
        owner: "akaidk",
        current_version: AdvancedSubmissionRequestedV1::SCHEMA_VERSION,
        minimum_supported_version: 1,
    },
    EventContractSpec {
        event_type: AdvancedSettlementCompletedV1::EVENT_TYPE,
        owner: "akaidk",
        current_version: AdvancedSettlementCompletedV1::SCHEMA_VERSION,
        minimum_supported_version: 1,
    },
    EventContractSpec {
        event_type: AdvancedCacheInvalidationRequestedV1::EVENT_TYPE,
        owner: "akaidk",
        current_version: AdvancedCacheInvalidationRequestedV1::SCHEMA_VERSION,
        minimum_supported_version: 1,
    },
    EventContractSpec {
        event_type: AdvancedSettlementDeadLetteredV1::EVENT_TYPE,
        owner: "akaidk",
        current_version: AdvancedSettlementDeadLetteredV1::SCHEMA_VERSION,
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

    /// Validates that an envelope still carries the contract implemented by
    /// its typed payload. Public envelope fields allow deserializers to fill
    /// the value, so consumers must perform this check before applying an
    /// effect.
    pub fn validate_contract(&self) -> Result<(), ContractError> {
        if self.event_type != T::EVENT_TYPE || self.schema_version != T::SCHEMA_VERSION {
            return Err(ContractError::UnsupportedEventVersion {
                event_type: self.event_type.clone(),
                version: self.schema_version,
            });
        }
        ensure_event_compatible(&self.event_type, self.schema_version)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedSubmissionRequestedV1 {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub question_ids: Vec<Uuid>,
    pub dedupe_key: String,
}

impl VersionedEvent for AdvancedSubmissionRequestedV1 {
    const EVENT_TYPE: &'static str = "orion.quiz.advanced.submitted";
    const SCHEMA_VERSION: u16 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedRatingEventV1 {
    pub event_id: Uuid,
    pub question_id: Uuid,
    pub correct: bool,
    pub rating_delta: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedSettlementCompletedV1 {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub rating_after: i32,
    pub events: Vec<AdvancedRatingEventV1>,
    pub dedupe_key: String,
}

impl VersionedEvent for AdvancedSettlementCompletedV1 {
    const EVENT_TYPE: &'static str = "orion.quiz.advanced.settled";
    const SCHEMA_VERSION: u16 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedCacheInvalidationRequestedV1 {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub question_ids: Vec<Uuid>,
    pub dedupe_key: String,
}

impl VersionedEvent for AdvancedCacheInvalidationRequestedV1 {
    const EVENT_TYPE: &'static str = "orion.quiz.cache.invalidate";
    const SCHEMA_VERSION: u16 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedSettlementDeadLetteredV1 {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
    pub dedupe_key: String,
}

impl VersionedEvent for AdvancedSettlementDeadLetteredV1 {
    const EVENT_TYPE: &'static str = "orion.quiz.advanced.settlement.dead_lettered";
    const SCHEMA_VERSION: u16 = 1;
}
