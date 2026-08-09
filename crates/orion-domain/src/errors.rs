use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContractError {
    #[error("rating cannot be negative: {0}")]
    NegativeRating(i32),
    #[error("unsupported version {version} for event {event_type}")]
    UnsupportedEventVersion { event_type: String, version: u16 },
}
