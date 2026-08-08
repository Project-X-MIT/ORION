use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// The starting Elo assigned to users and questions that have no history.
pub const DEFAULT_RATING: i32 = 1200;

/// The current rating and aggregate answer record for one user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct UserRating {
    pub user_id: Uuid,
    pub rating: i32,
    pub games_played: i32,
    pub wins: i32,
    pub losses: i32,
    pub draws: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The current Elo and aggregate answer record for one question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct QuestionRating {
    pub question_id: Uuid,
    pub rating: i32,
    pub attempts: i32,
    pub correct_answers: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Immutable audit data for one user-versus-question Elo update.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct RatingEvent {
    pub id: Uuid,
    pub attempt_id: Option<Uuid>,
    pub user_id: Uuid,
    pub question_id: Uuid,
    pub source_type: String,
    pub source_id: Uuid,
    pub quiz_type: String,
    pub outcome: i16,
    pub correct: bool,
    pub zone: String,
    pub error_pct: f64,
    pub k: i32,
    pub sa: f64,
    pub point_delta: i32,
    pub user_rating_before: i32,
    pub user_rating_after: i32,
    pub player_elo_before: i32,
    pub player_elo_after: i32,
    pub question_rating_before: i32,
    pub question_rating_after: i32,
    pub question_elo_before: i32,
    pub question_elo_after: i32,
    pub rating_delta: i32,
    pub created_at: DateTime<Utc>,
}
