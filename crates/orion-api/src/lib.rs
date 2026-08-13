//! HTTP composition owned by the platform team.
//!
//! Feature owners expose isolated routers; application assembly remains here.

use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
    Router,
};
use orion_common::{ApiError, ApiFailure, ApiSuccess, ErrorCode, RequestId};
use orion_db::error::DatabaseError;
use orion_redis::{RedisClientError, SessionStoreError};
use serde::Serialize;
use uuid::Uuid;

pub mod config;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod websocket;

#[derive(Debug, Clone)]
pub struct ApiProblem {
    pub status: axum::http::StatusCode,
    pub code: ErrorCode,
    pub message: &'static str,
    pub request_id: Option<RequestId>,
}

impl ApiProblem {
    #[must_use]
    pub const fn new(
        status: axum::http::StatusCode,
        code: ErrorCode,
        message: &'static str,
    ) -> Self {
        Self {
            status,
            code,
            message,
            request_id: None,
        }
    }

    #[must_use]
    pub const fn with_request_id(mut self, request_id: RequestId) -> Self {
        self.request_id = Some(request_id);
        self
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        let request_id = self
            .request_id
            .unwrap_or_else(|| RequestId::from_uuid(Uuid::new_v4()));
        let body = axum::Json(ApiFailure::new(
            request_id,
            ApiError::new(self.code, self.message),
        ));
        (self.status, body).into_response()
    }
}

impl From<DatabaseError> for ApiProblem {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::DuplicateEmail | DatabaseError::DuplicateUsername => Self::new(
                axum::http::StatusCode::CONFLICT,
                ErrorCode::Conflict,
                "account identifier is already in use",
            ),
            DatabaseError::Constraint(_) => Self::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                ErrorCode::ValidationFailed,
                "request could not be persisted",
            ),
            DatabaseError::Unavailable(_) => Self::new(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::ServiceUnavailable,
                "service temporarily unavailable",
            ),
            DatabaseError::Unexpected(_) => Self::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
                "internal server error",
            ),
        }
    }
}

impl From<SessionStoreError> for ApiProblem {
    fn from(_: SessionStoreError) -> Self {
        Self::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ServiceUnavailable,
            "session service temporarily unavailable",
        )
    }
}

impl From<RedisClientError> for ApiProblem {
    fn from(_: RedisClientError) -> Self {
        Self::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ServiceUnavailable,
            "session service temporarily unavailable",
        )
    }
}

#[must_use]
pub fn request_id(headers: &HeaderMap) -> RequestId {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(RequestId::from_uuid)
        .unwrap_or_else(|| RequestId::from_uuid(Uuid::new_v4()))
}

pub fn success<T: Serialize>(headers: &HeaderMap, data: T) -> axum::Json<ApiSuccess<T>> {
    axum::Json(ApiSuccess::new(request_id(headers), data))
}

pub fn app(state: state::AppState) -> Router {
    use axum::http::{header, HeaderValue, StatusCode};
    use tower::ServiceBuilder;
    use tower_http::{
        request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
        set_header::SetResponseHeaderLayer,
        timeout::TimeoutLayer,
        trace::TraceLayer,
    };

    let cors = middleware::cors::layer(&state.config.cors_origins);
    Router::new()
        .merge(routes::router())
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(TraceLayer::new_for_http().make_span_with(middleware::logging::span))
                .layer(cors)
                .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    state.config.request_timeout,
                )),
        )
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), geolocation=(), microphone=()"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}
