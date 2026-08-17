//! Pure domain contracts and feature business rules.
//!
//! This crate must not depend on HTTP, PostgreSQL, or Redis implementations.

pub mod elo;
pub mod entities;
pub mod errors;
pub mod events;
pub mod identity;
pub mod leaderboard;
pub mod learning;
pub mod profile;
pub mod quiz;
pub mod traits;
pub mod value_objects;

pub use entities::UserIdentity;
pub use errors::ContractError;
pub use events::{
    event_contract, AdvancedCacheInvalidationRequestedV1, AdvancedRatingEventV1,
    AdvancedSettlementCompletedV1, AdvancedSettlementDeadLetteredV1, AdvancedSubmissionRequestedV1,
    EventContractSpec, EventEnvelope, NotificationKind, NotificationRequestedV1, RatingReason,
    RatingUpdatedV1, EVENT_CONTRACTS,
};
pub use identity::{Identity, Role};
pub use profile::{
    PerformancePoint, ProfileDto, PublishedResearch, RankHistoryPoint, RatingHistoryPoint,
    PROFILE_SCHEMA_VERSION,
};
pub use traits::VersionedEvent;
pub use value_objects::{EventId, NotificationId, Rating, RatingEntryId, UserId};
