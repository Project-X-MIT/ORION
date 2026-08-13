use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Router};
use serde::Serialize;

use crate::{request_id, state::AppState, ApiProblem};

#[derive(Debug, Serialize)]
struct Liveness {
    service: &'static str,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct Readiness {
    service: &'static str,
    status: &'static str,
    dependencies: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(live))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
}

async fn live(headers: axum::http::HeaderMap) -> impl IntoResponse {
    crate::success(
        &headers,
        Liveness {
            service: "orion-api",
            status: "live",
        },
    )
}

async fn ready(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, ApiProblem> {
    if state.is_ready() && state.check_dependencies().await.is_ok() {
        return Ok(crate::success(
            &headers,
            Readiness {
                service: "orion-api",
                status: "ready",
                dependencies: "healthy",
            },
        ));
    }
    Err(ApiProblem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        orion_common::ErrorCode::ServiceUnavailable,
        "service dependencies are not ready",
    )
    .with_request_id(request_id(&headers)))
}
