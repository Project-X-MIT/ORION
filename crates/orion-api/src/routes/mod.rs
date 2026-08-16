pub mod auth;
pub mod health;
pub mod news;
pub mod quiz;
pub mod research;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/quiz", quiz::router())
        .nest("/api/v1/research", research::router())
    // TODO(Div): mount `news::router()` at `/api/v1/news` after the
    // shared route registry dependency is approved.
}
