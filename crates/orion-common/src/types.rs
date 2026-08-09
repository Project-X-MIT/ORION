use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(Uuid);

impl RequestId {
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    pub limit: u32,
    pub offset: u64,
}

pub const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PaginationError {
    #[error("page limit must be between 1 and {MAX_PAGE_SIZE}")]
    InvalidLimit,
}

impl PageRequest {
    pub const DEFAULT: Self = Self {
        limit: 20,
        offset: 0,
    };

    pub const fn new(limit: u32, offset: u64) -> Result<Self, PaginationError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(PaginationError::InvalidLimit);
        }
        Ok(Self { limit, offset })
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub limit: u32,
    pub offset: u64,
    pub total: u64,
}

impl<T> Page<T> {
    #[must_use]
    pub fn new(items: Vec<T>, request: PageRequest, total: u64) -> Self {
        Self {
            items,
            limit: request.limit,
            offset: request.offset,
            total,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiSuccess<T> {
    pub api_version: u16,
    pub request_id: RequestId,
    pub data: T,
}

impl<T> ApiSuccess<T> {
    pub const VERSION: u16 = 1;

    #[must_use]
    pub const fn new(request_id: RequestId, data: T) -> Self {
        Self {
            api_version: Self::VERSION,
            request_id,
            data,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl ApiMethod {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiAuth {
    Public,
    Authenticated,
    Reviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiOperationSpec {
    pub operation_id: &'static str,
    pub owner: &'static str,
    pub method: ApiMethod,
    pub path: &'static str,
    pub auth: ApiAuth,
    pub response_version: u16,
}

#[must_use]
pub fn api_operation(operation_id: &str) -> Option<&'static ApiOperationSpec> {
    API_OPERATIONS
        .iter()
        .find(|entry| entry.operation_id == operation_id)
}

macro_rules! operation {
    ($id:literal, $owner:literal, $method:ident, $path:literal, $auth:ident) => {
        ApiOperationSpec {
            operation_id: $id,
            owner: $owner,
            method: ApiMethod::$method,
            path: $path,
            auth: ApiAuth::$auth,
            response_version: 1,
        }
    };
}

pub const API_OPERATIONS: &[ApiOperationSpec] = &[
    operation!("health.get", "divi912", Get, "/health", Public),
    operation!(
        "auth.register",
        "divi912",
        Post,
        "/api/v1/auth/register",
        Public
    ),
    operation!("auth.login", "divi912", Post, "/api/v1/auth/login", Public),
    operation!(
        "auth.logout",
        "divi912",
        Post,
        "/api/v1/auth/logout",
        Authenticated
    ),
    operation!("auth.me", "divi912", Get, "/api/v1/auth/me", Authenticated),
    operation!(
        "notifications.list",
        "divi912",
        Get,
        "/api/v1/notifications",
        Authenticated
    ),
    operation!(
        "notifications.mark_read",
        "divi912",
        Patch,
        "/api/v1/notifications/{notification_id}",
        Authenticated
    ),
    operation!(
        "quiz.basic.get",
        "akaidk",
        Get,
        "/api/v1/quiz/basic",
        Authenticated
    ),
    operation!(
        "quiz.basic.submit",
        "akaidk",
        Post,
        "/api/v1/quiz/basic/attempts",
        Authenticated
    ),
    operation!(
        "quiz.advanced.get",
        "akaidk",
        Get,
        "/api/v1/quiz/advanced",
        Authenticated
    ),
    operation!(
        "quiz.advanced.submit",
        "akaidk",
        Post,
        "/api/v1/quiz/advanced/attempts",
        Authenticated
    ),
    operation!(
        "quiz.attempt.get",
        "akaidk",
        Get,
        "/api/v1/quiz/attempts/{attempt_id}",
        Authenticated
    ),
    operation!(
        "leaderboard.list",
        "ShauryaBijalwan",
        Get,
        "/api/v1/leaderboard",
        Public
    ),
    operation!(
        "profile.get",
        "ShauryaBijalwan",
        Get,
        "/api/v1/profiles/{user_id}",
        Public
    ),
    operation!(
        "research.create",
        "shivanshrawat13aug2007-commits",
        Post,
        "/api/v1/research",
        Authenticated
    ),
    operation!(
        "research.update",
        "shivanshrawat13aug2007-commits",
        Put,
        "/api/v1/research/{research_id}",
        Authenticated
    ),
    operation!(
        "research.submit",
        "shivanshrawat13aug2007-commits",
        Post,
        "/api/v1/research/{research_id}/submission",
        Authenticated
    ),
    operation!(
        "research.review",
        "shivanshrawat13aug2007-commits",
        Post,
        "/api/v1/research/{research_id}/reviews",
        Reviewer
    ),
    operation!(
        "research.list_published",
        "shivanshrawat13aug2007-commits",
        Get,
        "/api/v1/research",
        Public
    ),
    operation!(
        "research.get",
        "shivanshrawat13aug2007-commits",
        Get,
        "/api/v1/research/{research_id}",
        Public
    ),
    operation!("news.list", "sudhanshu001122", Get, "/api/v1/news", Public),
    operation!(
        "learning.course.get",
        "sudhanshu001122",
        Get,
        "/api/v1/learning/courses/{course_id}",
        Public
    ),
    operation!(
        "learning.progress.get",
        "sudhanshu001122",
        Get,
        "/api/v1/learning/progress",
        Authenticated
    ),
    operation!(
        "learning.lesson.complete",
        "sudhanshu001122",
        Post,
        "/api/v1/learning/lessons/{lesson_id}/completion",
        Authenticated
    ),
    operation!(
        "discord.connect",
        "sudhanshu001122",
        Get,
        "/api/v1/discord/connect",
        Public
    ),
];
