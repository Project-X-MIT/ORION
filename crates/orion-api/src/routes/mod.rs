pub mod auth;
pub mod health;
<<<<<<< HEAD
pub mod news;
=======
pub mod leaderboard;
pub mod metrics;
pub mod notification;
>>>>>>> 6bf1d4712e3af9ccf5a26f62f3f86dbd2b657878
pub mod quiz;
pub mod research;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(health::router())
        .merge(metrics::router())
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/leaderboard", leaderboard::router())
        .nest("/api/v1/quiz", quiz::router())
        .nest("/api/v1/notifications", notification::router())
        .nest("/api/v1/research", research::router())
<<<<<<< HEAD
    // TODO(Div): mount `news::router()` at `/api/v1/news` after the
    // shared route registry dependency is approved.
=======
        .merge(crate::websocket::gateway::router())
>>>>>>> 6bf1d4712e3af9ccf5a26f62f3f86dbd2b657878
}
