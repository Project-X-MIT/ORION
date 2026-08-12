use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ContractError;

// Compatibility re-export for consumers that adopted the initial YASH-03
// module path. The policy itself is owned by the single top-level domain
// module in `src/elo.rs`.
pub use crate::elo;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_id!(UserId);
uuid_id!(EventId);
uuid_id!(NotificationId);
uuid_id!(RatingEntryId);

/// Authoritative whole-number Elo rating transported between features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rating(i32);

impl Rating {
    pub const fn new(value: i32) -> Result<Self, ContractError> {
        if value < 0 {
            return Err(ContractError::NegativeRating(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Display for Rating {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
