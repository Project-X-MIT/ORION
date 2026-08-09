use serde::{Deserialize, Serialize};

use crate::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Reviewer,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub user_id: UserId,
    pub role: Role,
}
