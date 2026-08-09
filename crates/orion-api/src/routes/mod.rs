pub mod auth;
pub mod health;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/api/v1/auth", auth::router())
}
