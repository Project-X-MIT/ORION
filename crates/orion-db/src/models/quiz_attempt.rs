use chrono::{DateTime, Utc};
use orion_domain::quiz::{AdvancedActualValue, AdvancedPrediction};
use sqlx::FromRow;
use uuid::Uuid;

use super::quiz_question::QuizType;
use super::rating::{RatingEvent, UserRating};

/// A submitted answer. `None` represents an unanswered question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuizAnswer {
    pub question_id: Uuid,
    pub option_id: Option<Uuid>,
}

impl QuizAnswer {
    pub const fn selected(question_id: Uuid, option_id: Uuid) -> Self {
        Self {
            question_id,
            option_id: Some(option_id),
        }
    }

    pub const fn unanswered(question_id: Uuid) -> Self {
        Self {
            question_id,
            option_id: None,
        }
    }
}

/// A quiz submission and its settled score/rating summary.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct QuizAttempt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub quiz_type: String,
    pub status: String,
    pub total_questions: i32,
    pub correct_answers: i32,
    pub score: i32,
    pub rating_before: i32,
    pub rating_after: i32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a pending attempt before answers are submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuizAttempt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub quiz_type: QuizType,
    pub total_questions: i32,
    pub rating_before: i32,
    pub started_at: DateTime<Utc>,
}

impl NewQuizAttempt {
    pub fn new(
        user_id: Uuid,
        quiz_type: QuizType,
        total_questions: i32,
        rating_before: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            quiz_type,
            total_questions,
            rating_before,
            started_at: Utc::now(),
        }
    }
}

/// Input shared by basic and advanced atomic settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizSettlementInput {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub answers: Vec<QuizAnswer>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

impl QuizSettlementInput {
    pub fn new(user_id: Uuid, answers: Vec<QuizAnswer>) -> Self {
        let now = Utc::now();
        Self {
            attempt_id: Uuid::new_v4(),
            user_id,
            answers,
            started_at: now,
            completed_at: now,
        }
    }

    pub fn for_attempt(attempt_id: Uuid, user_id: Uuid, answers: Vec<QuizAnswer>) -> Self {
        let now = Utc::now();
        Self {
            attempt_id,
            user_id,
            answers,
            started_at: now,
            completed_at: now,
        }
    }
}

/// The complete result returned after a settlement commits.
#[derive(Debug, Clone, PartialEq)]
pub struct QuizSettlementResult {
    pub attempt: QuizAttempt,
    pub user_rating: UserRating,
    pub events: Vec<RatingEvent>,
}

/// One validated prediction/actual pair passed to the atomic Advanced
/// settlement transaction. The worker owns provider access and validation;
/// PostgreSQL owns scoring, Elo updates, idempotency, and persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedSettlementResolution {
    pub prediction: AdvancedPrediction,
    pub actual: AdvancedActualValue,
}

/// Input for the actual-value-driven Advanced settlement path.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedSettlementInput {
    pub attempt_id: Uuid,
    pub user_id: Uuid,
    pub resolutions: Vec<AdvancedSettlementResolution>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

/// Convenience alias for the Basic settlement API.
pub type BasicSettlementInput = QuizSettlementInput;

pub const ATTEMPT_PENDING: &str = "pending";
pub const ATTEMPT_COMPLETED: &str = "completed";

pub fn quiz_type_name(quiz_type: QuizType) -> &'static str {
    quiz_type.as_str()
}
