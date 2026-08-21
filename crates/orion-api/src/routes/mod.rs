pub mod auth;
pub mod discord;
pub mod health;
pub mod leaderboard;
pub mod learning;
pub mod metrics;
pub mod news;
pub mod notification;
pub mod profile;
pub mod quiz;
pub mod research;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(metrics::router())
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/discord", discord::router())
        .nest("/api/v1/leaderboard", leaderboard::router())
        .nest("/api/v1/learning", learning::router())
        .nest("/api/v1/quiz", quiz::router())
        .nest("/api/v1/notifications", notification::router())
        .nest("/api/v1/profiles", profile::router())
        .nest("/api/v1/research", research::router())
        .nest("/api/v1/news", news::router())
        .merge(crate::websocket::gateway::router())
}
