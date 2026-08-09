use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Disabled,
    Deleted,
}

impl UserStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Deleted => "deleted",
        }
    }
}

impl fmt::Display for UserStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidUserStatus;

impl fmt::Display for InvalidUserStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid user status")
    }
}

impl std::error::Error for InvalidUserStatus {}

impl FromStr for UserStatus {
    type Err = InvalidUserStatus;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            "deleted" => Ok(Self::Deleted),
            _ => Err(InvalidUserStatus),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub status: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn parsed_status(&self) -> Result<UserStatus, InvalidUserStatus> {
        self.status.parse()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub username: &'a str,
    pub password_hash: &'a str,
    pub display_name: Option<&'a str>,
}
