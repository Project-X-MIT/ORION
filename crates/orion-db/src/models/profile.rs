use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// Public profile data assembled from the shared user record and user activity.
///
/// This is a read model rather than a one-to-one representation of a table. It
/// deliberately excludes private authentication data and can be populated by a
/// query joining `users` with rating and quiz aggregates.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Profile {
    pub user_id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub rating: Option<i32>,
    pub global_rank: Option<i64>,
    pub quizzes_completed: i64,
    pub correct_answers: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Aggregate statistics displayed alongside a user's profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRow)]
pub struct ProfileStatistics {
    pub user_id: Uuid,
    pub rating: Option<i32>,
    pub global_rank: Option<i64>,
    pub quizzes_completed: i64,
    pub correct_answers: i64,
}

/// A completed quiz observation used by the public performance chart.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProfilePerformanceRow {
    pub completed_at: DateTime<Utc>,
    pub quiz_type: String,
    pub total_questions: i32,
    pub correct_answers: i32,
    pub score: i32,
    pub rating_after: i32,
}

/// The deliberately small public projection of a published research paper.
/// Content and reviewer/evaluation payloads are not selected here.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PublishedProfileResearchRow {
    pub id: Uuid,
    pub title: String,
    pub abstract_text: String,
    pub published_at: DateTime<Utc>,
    pub evaluation_score: Option<f64>,
    pub evaluated_content_version: Option<i32>,
    pub elo_award: Option<i32>,
    pub elo_awarded: bool,
}
