use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MIN_INTENDED_SECONDS: u8 = 15;
pub const MAX_INTENDED_SECONDS: u8 = 30;

/// A specification topic used to group related Basic Quiz questions.
///
/// Topics remain data-driven because the specification may grow without
/// requiring a domain code change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpecificationTopic(String);

impl SpecificationTopic {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SpecificationTopic {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SpecificationTopic {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Intended solving time for one Basic Quiz question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct IntendedQuestionSeconds(u8);

impl IntendedQuestionSeconds {
    /// Creates a duration accepted by the Basic Quiz specification.
    pub const fn new(seconds: u8) -> Result<Self, InvalidIntendedQuestionSeconds> {
        if seconds < MIN_INTENDED_SECONDS || seconds > MAX_INTENDED_SECONDS {
            return Err(InvalidIntendedQuestionSeconds { seconds });
        }
        Ok(Self(seconds))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for IntendedQuestionSeconds {
    type Error = InvalidIntendedQuestionSeconds;

    fn try_from(seconds: u8) -> Result<Self, Self::Error> {
        Self::new(seconds)
    }
}

impl<'de> Deserialize<'de> for IntendedQuestionSeconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = u8::deserialize(deserializer)?;
        Self::new(seconds).map_err(|error| {
            serde::de::Error::custom(format!(
                "intended question duration must be between {MIN_INTENDED_SECONDS} and {MAX_INTENDED_SECONDS} seconds, got {}",
                error.seconds
            ))
        })
    }
}

/// Returned when an intended Basic Quiz duration is outside the 15-30 second
/// range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidIntendedQuestionSeconds {
    pub seconds: u8,
}

/// A multiple-choice question presented by the Basic Quiz.
///
/// The answer key is represented by [`McqOption::is_correct`] on the domain
/// model. Callers that expose a question to a client should project options
/// without that field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McqQuestion {
    pub id: Uuid,
    pub topic: SpecificationTopic,
    pub prompt: String,
    pub options: Vec<McqOption>,
    pub intended_seconds: IntendedQuestionSeconds,
}

/// One selectable option belonging to an MCQ question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McqOption {
    pub id: Uuid,
    pub text: String,
    pub is_correct: bool,
}

/// A user's answer to one MCQ question.
///
/// `None` represents an unanswered question and lets submission validation
/// decide whether unanswered questions are allowed for a particular quiz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McqAnswer {
    pub question_id: Uuid,
    pub selected_option_id: Option<Uuid>,
}

/// The deterministic result of evaluating one MCQ answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McqResult {
    pub question_id: Uuid,
    pub selected_option_id: Option<Uuid>,
    pub correct_option_id: Uuid,
    pub is_correct: bool,
}

/// The immutable request passed from the Basic Quiz domain to settlement
/// persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BasicSettlementRequest {
    attempt_id: Uuid,
    user_id: Uuid,
    answers: Vec<McqAnswer>,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

impl BasicSettlementRequest {
    #[must_use]
    pub const fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }

    #[must_use]
    pub const fn user_id(&self) -> Uuid {
        self.user_id
    }

    #[must_use]
    pub fn answers(&self) -> &[McqAnswer] {
        &self.answers
    }

    #[must_use]
    pub const fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    #[must_use]
    pub const fn completed_at(&self) -> DateTime<Utc> {
        self.completed_at
    }

    /// Creates a new request with a generated attempt ID and current UTC time.
    pub fn new(
        user_id: Uuid,
        answers: Vec<McqAnswer>,
    ) -> Result<Self, BasicSettlementRequestError> {
        let now = Utc::now();
        Self::for_attempt(Uuid::new_v4(), user_id, answers, now, now)
    }

    /// Creates a request for an existing attempt after validating its answers
    /// and timestamps.
    pub fn for_attempt(
        attempt_id: Uuid,
        user_id: Uuid,
        answers: Vec<McqAnswer>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Result<Self, BasicSettlementRequestError> {
        let request = Self {
            attempt_id,
            user_id,
            answers,
            started_at,
            completed_at,
        };
        request.validate()?;

        // Settlement locks question ratings in UUID order. Keeping the domain
        // request in the same order makes retries deterministic as well.
        let mut answers = request.answers;
        answers.sort_unstable_by_key(|answer| answer.question_id);

        Ok(Self {
            attempt_id,
            user_id,
            answers,
            started_at,
            completed_at,
        })
    }

    /// Revalidates a request after construction or deserialization.
    pub fn validate(&self) -> Result<(), BasicSettlementRequestError> {
        if self.answers.is_empty() {
            return Err(BasicSettlementRequestError::EmptyAnswers);
        }
        if self.completed_at < self.started_at {
            return Err(BasicSettlementRequestError::CompletionBeforeStart);
        }

        let mut seen_questions = HashSet::with_capacity(self.answers.len());
        for answer in &self.answers {
            if answer.selected_option_id.is_none() {
                return Err(BasicSettlementRequestError::NoSelectedAnswer {
                    question_id: answer.question_id,
                });
            }
            if !seen_questions.insert(answer.question_id) {
                return Err(BasicSettlementRequestError::DuplicateQuestion {
                    question_id: answer.question_id,
                });
            }
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct BasicSettlementRequestWire {
    attempt_id: Uuid,
    user_id: Uuid,
    answers: Vec<McqAnswer>,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

impl<'de> Deserialize<'de> for BasicSettlementRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let request = BasicSettlementRequestWire::deserialize(deserializer)?;
        Self::for_attempt(
            request.attempt_id,
            request.user_id,
            request.answers,
            request.started_at,
            request.completed_at,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Rejected Basic Quiz settlement request inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BasicSettlementRequestError {
    #[error("a Basic Quiz settlement requires at least one answer")]
    EmptyAnswers,

    #[error("question {question_id} has no selected answer")]
    NoSelectedAnswer { question_id: Uuid },

    #[error("question {question_id} appears more than once in the settlement")]
    DuplicateQuestion { question_id: Uuid },

    #[error("settlement completion cannot be earlier than its start")]
    CompletionBeforeStart,
}

/// Creates a validated Basic Quiz settlement request for an existing attempt.
pub fn create_settlement_request(
    attempt_id: Uuid,
    user_id: Uuid,
    answers: Vec<McqAnswer>,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
) -> Result<BasicSettlementRequest, BasicSettlementRequestError> {
    BasicSettlementRequest::for_attempt(attempt_id, user_id, answers, started_at, completed_at)
}

/// Basic Quiz names for the generic MCQ domain contracts.
pub type BasicQuestion = McqQuestion;
pub type BasicOption = McqOption;
pub type BasicAnswer = McqAnswer;
pub type BasicResult = McqResult;
pub type BasicSettlementInput = BasicSettlementRequest;

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use super::{IntendedQuestionSeconds, MAX_INTENDED_SECONDS, MIN_INTENDED_SECONDS};

    #[test]
    fn intended_duration_accepts_specification_boundaries() {
        assert_eq!(
            IntendedQuestionSeconds::new(MIN_INTENDED_SECONDS)
                .expect("minimum duration is valid")
                .get(),
            MIN_INTENDED_SECONDS
        );
        assert_eq!(
            IntendedQuestionSeconds::new(MAX_INTENDED_SECONDS)
                .expect("maximum duration is valid")
                .get(),
            MAX_INTENDED_SECONDS
        );
    }

    #[test]
    fn intended_duration_rejects_values_outside_specification_range() {
        assert!(IntendedQuestionSeconds::new(MIN_INTENDED_SECONDS - 1).is_err());
        assert!(IntendedQuestionSeconds::new(MAX_INTENDED_SECONDS + 1).is_err());
    }

    #[test]
    fn intended_duration_deserialization_preserves_bounds() {
        assert_eq!(
            serde_json::from_str::<IntendedQuestionSeconds>("15")
                .expect("minimum duration deserializes")
                .get(),
            MIN_INTENDED_SECONDS
        );
        assert_eq!(
            serde_json::from_str::<IntendedQuestionSeconds>("30")
                .expect("maximum duration deserializes")
                .get(),
            MAX_INTENDED_SECONDS
        );
        assert!(serde_json::from_str::<IntendedQuestionSeconds>("14").is_err());
        assert!(serde_json::from_str::<IntendedQuestionSeconds>("31").is_err());
    }

    #[test]
    fn property_duration_acceptance_matches_specification_bounds_for_all_u8_inputs() {
        for seconds in u8::MIN..=u8::MAX {
            let expected = (MIN_INTENDED_SECONDS..=MAX_INTENDED_SECONDS).contains(&seconds);
            assert_eq!(
                IntendedQuestionSeconds::new(seconds).is_ok(),
                expected,
                "unexpected duration acceptance for {seconds} seconds"
            );
        }
    }

    #[test]
    fn settlement_request_sorts_answers_and_preserves_attempt_metadata() {
        let started_at = Utc::now();
        let completed_at = started_at + Duration::seconds(20);
        let request = super::BasicSettlementRequest::for_attempt(
            Uuid::from_u128(100),
            Uuid::from_u128(200),
            vec![
                super::McqAnswer {
                    question_id: Uuid::from_u128(3),
                    selected_option_id: Some(Uuid::from_u128(30)),
                },
                super::McqAnswer {
                    question_id: Uuid::from_u128(2),
                    selected_option_id: Some(Uuid::from_u128(20)),
                },
            ],
            started_at,
            completed_at,
        )
        .expect("valid settlement request");

        assert_eq!(request.attempt_id(), Uuid::from_u128(100));
        assert_eq!(request.user_id(), Uuid::from_u128(200));
        assert_eq!(request.answers()[0].question_id, Uuid::from_u128(2));
        assert_eq!(request.answers()[1].question_id, Uuid::from_u128(3));
        assert_eq!(request.started_at(), started_at);
        assert_eq!(request.completed_at(), completed_at);
    }

    #[test]
    fn settlement_request_rejects_empty_duplicate_unanswered_and_invalid_time() {
        let now = Utc::now();
        let question_id = Uuid::from_u128(2);
        let answer = super::McqAnswer {
            question_id,
            selected_option_id: Some(Uuid::from_u128(20)),
        };

        assert!(matches!(
            super::BasicSettlementRequest::for_attempt(
                Uuid::from_u128(100),
                Uuid::from_u128(200),
                Vec::new(),
                now,
                now,
            ),
            Err(super::BasicSettlementRequestError::EmptyAnswers)
        ));
        assert!(matches!(
            super::BasicSettlementRequest::for_attempt(
                Uuid::from_u128(100),
                Uuid::from_u128(200),
                vec![answer, answer],
                now,
                now,
            ),
            Err(super::BasicSettlementRequestError::DuplicateQuestion { .. })
        ));
        assert!(matches!(
            super::BasicSettlementRequest::for_attempt(
                Uuid::from_u128(100),
                Uuid::from_u128(200),
                vec![super::McqAnswer {
                    question_id,
                    selected_option_id: None,
                }],
                now,
                now,
            ),
            Err(super::BasicSettlementRequestError::NoSelectedAnswer { .. })
        ));
        assert!(matches!(
            super::BasicSettlementRequest::for_attempt(
                Uuid::from_u128(100),
                Uuid::from_u128(200),
                vec![answer],
                now,
                now - Duration::seconds(1),
            ),
            Err(super::BasicSettlementRequestError::CompletionBeforeStart)
        ));
    }

    #[test]
    fn property_settlement_request_order_is_canonical_for_permutations() {
        let started_at = Utc::now();
        for length in 1..=12 {
            let answers = (0..length)
                .rev()
                .map(|question| super::McqAnswer {
                    question_id: Uuid::from_u128(question as u128 + 1),
                    selected_option_id: Some(Uuid::from_u128(question as u128 + 101)),
                })
                .collect();
            let request = super::BasicSettlementRequest::for_attempt(
                Uuid::from_u128(length as u128),
                Uuid::from_u128(200),
                answers,
                started_at,
                started_at + Duration::seconds(length as i64),
            )
            .expect("permuted valid answers");

            let question_ids: Vec<_> = request
                .answers()
                .iter()
                .map(|answer| answer.question_id)
                .collect();
            let mut sorted_ids = question_ids.clone();
            sorted_ids.sort_unstable();
            assert_eq!(question_ids, sorted_ids);
        }
    }

    #[test]
    fn malformed_or_duplicate_submissions_cannot_create_a_settlement_request() {
        let now = Utc::now();
        let duplicate_answers = vec![
            super::McqAnswer {
                question_id: Uuid::from_u128(1),
                selected_option_id: Some(Uuid::from_u128(2)),
            },
            super::McqAnswer {
                question_id: Uuid::from_u128(1),
                selected_option_id: Some(Uuid::from_u128(3)),
            },
        ];

        assert!(super::create_settlement_request(
            Uuid::from_u128(10),
            Uuid::from_u128(20),
            Vec::new(),
            now,
            now,
        )
        .is_err());
        assert!(super::create_settlement_request(
            Uuid::from_u128(10),
            Uuid::from_u128(20),
            duplicate_answers,
            now,
            now,
        )
        .is_err());

        let malformed_json = format!(
            r#"{{
                "attempt_id": "{}",
                "user_id": "{}",
                "answers": [],
                "started_at": "{}",
                "completed_at": "{}"
            }}"#,
            Uuid::from_u128(10),
            Uuid::from_u128(20),
            now.to_rfc3339(),
            now.to_rfc3339(),
        );
        assert!(serde_json::from_str::<super::BasicSettlementRequest>(&malformed_json).is_err());

        let duplicate_json = format!(
            r#"{{
                "attempt_id": "{}",
                "user_id": "{}",
                "answers": [
                    {{
                        "question_id": "{}",
                        "selected_option_id": "{}"
                    }},
                    {{
                        "question_id": "{}",
                        "selected_option_id": "{}"
                    }}
                ],
                "started_at": "{}",
                "completed_at": "{}"
            }}"#,
            Uuid::from_u128(10),
            Uuid::from_u128(20),
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(1),
            Uuid::from_u128(3),
            now.to_rfc3339(),
            now.to_rfc3339(),
        );
        assert!(serde_json::from_str::<super::BasicSettlementRequest>(&duplicate_json).is_err());
    }
}
