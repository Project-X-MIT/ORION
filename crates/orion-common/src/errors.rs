use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RequestId;

/// Stable machine-readable API errors. Variants may be added compatibly, but
/// existing serialized names must not be changed within API version 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidRequest,
    ValidationFailed,
    Unauthenticated,
    Forbidden,
    NotFound,
    Conflict,
    RateLimited,
    ServiceUnavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

impl ApiError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, field: impl Into<String>, message: impl Into<String>) -> Self {
        self.details.insert(field.into(), message.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiFailure {
    pub api_version: u16,
    pub request_id: RequestId,
    pub error: ApiError,
}

impl ApiFailure {
    pub const VERSION: u16 = 1;

    #[must_use]
    pub const fn new(request_id: RequestId, error: ApiError) -> Self {
        Self {
            api_version: Self::VERSION,
            request_id,
            error,
        }
    }
}
