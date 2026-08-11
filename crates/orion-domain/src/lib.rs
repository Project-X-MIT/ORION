//! Pure domain contracts and feature business rules.
//!
//! This crate must not depend on HTTP, PostgreSQL, or Redis implementations.

pub mod entities;
pub mod errors;
pub mod events;
pub mod identity;
pub mod leaderboard;
pub mod traits;
pub mod value_objects;

pub use entities::UserIdentity;
pub use errors::ContractError;
pub use events::{
    event_contract, EventContractSpec, EventEnvelope, NotificationKind, NotificationRequestedV1,
    RatingReason, RatingUpdatedV1, EVENT_CONTRACTS,
};
pub use identity::{Identity, Role};
pub use traits::VersionedEvent;
pub use value_objects::{EventId, NotificationId, Rating, RatingEntryId, UserId};
