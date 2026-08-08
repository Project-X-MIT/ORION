use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::rating::QuestionRating;

/// A quiz mode understood by the persistence layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuizType {
    Basic,
    Advanced,
}

impl QuizType {
    pub const BASIC: &'static str = "basic";
    pub const ADVANCED: &'static str = "advanced";

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Basic => Self::BASIC,
            Self::Advanced => Self::ADVANCED,
        }
    }
}

impl AsRef<str> for QuizType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// A question without its answer options.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct QuizQuestion {
    pub id: Uuid,
    pub quiz_type: String,
    pub category: String,
    pub question_text: String,
    pub explanation: Option<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One selectable answer belonging to a question.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct QuizOption {
    pub id: Uuid,
    pub question_id: Uuid,
    pub option_text: String,
    pub position: i32,
    pub is_correct: bool,
    pub created_at: DateTime<Utc>,
}

/// A question and all of its options, used by quiz read endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizQuestionWithOptions {
    pub question: QuizQuestion,
    pub options: Vec<QuizOption>,
    pub rating: Option<QuestionRating>,
}

/// The safe option projection for clients that must not receive the answer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicQuizOption {
    pub id: Uuid,
    pub option_text: String,
    pub position: i32,
}

impl From<&QuizOption> for PublicQuizOption {
    fn from(option: &QuizOption) -> Self {
        Self {
            id: option.id,
            option_text: option.option_text.clone(),
            position: option.position,
        }
    }
}

/// Data required to create a question and its options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuizQuestion {
    pub id: Uuid,
    pub quiz_type: QuizType,
    pub category: String,
    pub question_text: String,
    pub explanation: Option<String>,
    pub options: Vec<NewQuizOption>,
}

/// Data required to create one option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewQuizOption {
    pub id: Uuid,
    pub option_text: String,
    pub position: i32,
    pub is_correct: bool,
}
