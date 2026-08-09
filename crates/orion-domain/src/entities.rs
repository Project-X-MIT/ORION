use serde::{Deserialize, Serialize};

use crate::UserId;

/// Minimum stable user representation shared between feature boundaries.
/// Private profile data and persistence-specific columns are intentionally
/// excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentity {
    pub id: UserId,
    pub username: String,
}
