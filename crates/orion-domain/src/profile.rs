//! Public profile read-model contracts.
//!
//! A profile is deliberately a public projection.  Authentication material,
//! email addresses, lifecycle state, private research drafts, and reviewer
//! identity never have a field in these DTOs, which makes accidental leakage
//! through PostgreSQL or a stale Redis value impossible at the type boundary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Rating, UserId};

pub const PROFILE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileDto {
    pub schema_version: u16,
    pub user_id: UserId,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub rating: Option<Rating>,
    pub global_rank: Option<u64>,
    pub rank_movement: Option<i64>,
    pub quizzes_completed: u64,
    pub correct_answers: u64,
    pub rating_history: Vec<RatingHistoryPoint>,
    pub rank_history: Vec<RankHistoryPoint>,
    pub performance_history: Vec<PerformancePoint>,
    pub published_research: Vec<PublishedResearch>,
}

impl ProfileDto {
    /// Trims a cached complete projection to the caller's requested history
    /// window. The cache key intentionally does not vary by limit, so the
    /// API caches one bounded projection and applies this response-only view.
    pub fn truncate_history(&mut self, limit: usize) {
        self.rating_history.truncate(limit);
        self.rank_history.truncate(limit);
        self.performance_history.truncate(limit);
        self.published_research.truncate(limit);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingHistoryPoint {
    pub occurred_at: DateTime<Utc>,
    pub quiz_type: String,
    pub rating_before: Rating,
    pub rating_after: Rating,
    pub rating_delta: i32,
    pub correct: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RankHistoryPoint {
    pub snapshot_at: DateTime<Utc>,
    pub previous_rank: Option<u64>,
    pub current_rank: u64,
    pub rank_movement: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformancePoint {
    pub completed_at: DateTime<Utc>,
    pub quiz_type: String,
    pub total_questions: u32,
    pub correct_answers: u32,
    pub score: u32,
    pub rating_after: Rating,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishedResearch {
    pub id: uuid::Uuid,
    pub title: String,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub published_at: DateTime<Utc>,
    pub evaluation_score: Option<f64>,
    pub evaluated_content_version: Option<u32>,
    pub elo_award: Option<i32>,
    pub elo_awarded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use uuid::Uuid;

    #[test]
    fn public_profile_serialization_has_no_private_identity_fields() {
        let profile = ProfileDto {
            schema_version: PROFILE_SCHEMA_VERSION,
            user_id: UserId::from_uuid(Uuid::nil()),
            username: "orion".to_owned(),
            display_name: None,
            bio: None,
            avatar_url: None,
            rating: None,
            global_rank: None,
            rank_movement: None,
            quizzes_completed: 0,
            correct_answers: 0,
            rating_history: Vec::new(),
            rank_history: Vec::new(),
            performance_history: Vec::new(),
            published_research: Vec::new(),
        };
        let value = serde_json::to_value(profile).expect("profile serializes");
        let object = value.as_object().expect("object response");
        for private_field in ["email", "password_hash", "status", "decided_by", "content"] {
            assert!(!object.contains_key(private_field));
        }
        assert_eq!(value["schema_version"], Value::from(1));
    }
}
