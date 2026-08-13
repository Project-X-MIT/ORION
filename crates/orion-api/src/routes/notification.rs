use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use orion_common::{ErrorCode, MAX_PAGE_SIZE};
use orion_db::{models::Notification, transactions};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{routes::auth::AuthenticatedUser, state::AppState, ApiProblem};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/unread-count", get(unread_count))
        .route("/{notification_id}/read", patch(mark_read))
}

#[derive(Debug, Deserialize)]
struct Page {
    limit: Option<u32>,
    offset: Option<u64>,
}

#[derive(Debug, Serialize)]
struct NotificationPage {
    items: Vec<Notification>,
    limit: u32,
    offset: u64,
}

#[derive(Debug, Serialize)]
struct UnreadCount {
    unread_count: i64,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(page): Query<Page>,
) -> Result<Json<NotificationPage>, ApiProblem> {
    let limit = page.limit.unwrap_or(20).clamp(1, MAX_PAGE_SIZE);
    let offset = page.offset.unwrap_or(0);
    let items = transactions::list_notifications(
        &state.db,
        auth.user.id,
        i64::from(limit),
        i64::try_from(offset).unwrap_or(i64::MAX),
    )
    .await?;
    Ok(Json(NotificationPage {
        items,
        limit,
        offset,
    }))
}

async fn unread_count(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<UnreadCount>, ApiProblem> {
    let unread_count = transactions::unread_notification_count(&state.db, auth.user.id).await?;
    Ok(Json(UnreadCount { unread_count }))
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<Notification>, ApiProblem> {
    transactions::mark_notification_read(&state.db, auth.user.id, notification_id)
        .await?
        .map(Json)
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ErrorCode::NotFound,
                "notification was not found",
            )
        })
}
