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
