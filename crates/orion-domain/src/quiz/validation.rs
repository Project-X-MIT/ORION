use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

use super::basic::{
    BasicSettlementRequest, BasicSettlementRequestError, McqAnswer, McqOption, McqQuestion,
};

/// Maximum time allotted per Basic Quiz question for settlement acceptance.
pub const MAX_SUBMISSION_SECONDS_PER_QUESTION: i64 = 30;

/// Validation failures for Basic Quiz questions and answers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum McqValidationError {
    #[error("question {question_id} must have one eligible answer, but has none")]
    NoEligibleAnswer { question_id: Uuid },

    #[error("question {question_id} must have one eligible answer, but has {count}")]
    MultipleEligibleAnswers { question_id: Uuid, count: usize },

    #[error("answer targets question {actual_question_id}, expected {expected_question_id}")]
    QuestionMismatch {
        expected_question_id: Uuid,
        actual_question_id: Uuid,
    },

    #[error("question {question_id} has no selected answer")]
    NoSelectedAnswer { question_id: Uuid },

    #[error("option {option_id} is not an option for question {question_id}")]
    OptionNotFound { question_id: Uuid, option_id: Uuid },

    #[error("option {option_id} occurs more than once for question {question_id}")]
    DuplicateOption { question_id: Uuid, option_id: Uuid },
}

/// Returns the one eligible option for a question.
pub fn eligible_option(question: &McqQuestion) -> Result<&McqOption, McqValidationError> {
    let eligible = question.options.iter().filter(|option| option.is_correct);
    let mut eligible = eligible;
    let Some(option) = eligible.next() else {
        return Err(McqValidationError::NoEligibleAnswer {
            question_id: question.id,
        });
    };

    if eligible.next().is_some() {
        let count = question
            .options
            .iter()
            .filter(|option| option.is_correct)
            .count();
        return Err(McqValidationError::MultipleEligibleAnswers {
            question_id: question.id,
            count,
        });
    }

    Ok(option)
}

/// Validates the answer key and returns its unique eligible option ID.
pub fn validate_question(question: &McqQuestion) -> Result<Uuid, McqValidationError> {
    let mut seen = std::collections::HashSet::with_capacity(question.options.len());
    for option in &question.options {
        if !seen.insert(option.id) {
            return Err(McqValidationError::DuplicateOption {
                question_id: question.id,
                option_id: option.id,
            });
        }
    }

    Ok(eligible_option(question)?.id)
}

/// Validates that an answer selects exactly one option belonging to the
/// question. A wrong option is valid; correctness is determined by scoring.
pub fn validate_answer(
    question: &McqQuestion,
    answer: McqAnswer,
) -> Result<(), McqValidationError> {
    if answer.question_id != question.id {
        return Err(McqValidationError::QuestionMismatch {
            expected_question_id: question.id,
            actual_question_id: answer.question_id,
        });
    }

    let Some(option_id) = answer.selected_option_id else {
        return Err(McqValidationError::NoSelectedAnswer {
            question_id: question.id,
        });
    };

    validate_question(question)?;

    let matching_options = question
        .options
        .iter()
        .filter(|option| option.id == option_id)
        .count();
    match matching_options {
        0 => Err(McqValidationError::OptionNotFound {
            question_id: question.id,
            option_id,
        }),
        1 => Ok(()),
        _ => Err(McqValidationError::DuplicateOption {
            question_id: question.id,
            option_id,
        }),
    }
}

/// Alias for callers that prefer validation terminology over answer-key
/// terminology.
pub type ValidationError = McqValidationError;

/// Submission-level failures that are distinct from answer-key validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SubmissionValidationError {
    #[error("malformed Basic Quiz submission: {0}")]
    Malformed(#[from] BasicSettlementRequestError),

    #[error("Basic Quiz submission for attempt {attempt_id} has expired at {expires_at}")]
    Expired {
        attempt_id: Uuid,
        expires_at: DateTime<Utc>,
    },

    #[error(
        "Basic Quiz submission for attempt {attempt_id} was received before its completion time"
    )]
    CompletionInFuture { attempt_id: Uuid },

    #[error("Basic Quiz attempt {attempt_id} has already been submitted")]
    Duplicate { attempt_id: Uuid },
}

/// Calculates the acceptance deadline from the number of submitted questions.
pub fn submission_expires_at(request: &BasicSettlementRequest) -> DateTime<Utc> {
    let question_count = i64::try_from(request.answers().len()).unwrap_or(i64::MAX);
    let seconds = question_count.saturating_mul(MAX_SUBMISSION_SECONDS_PER_QUESTION);
    request.started_at() + Duration::seconds(seconds)
}

/// Validates a request against the server's acceptance time and duplicate
/// state. Persistence should perform the same check while claiming the
/// attempt transactionally.
pub fn validate_submission(
    request: &BasicSettlementRequest,
    accepted_at: DateTime<Utc>,
    already_submitted: bool,
) -> Result<(), SubmissionValidationError> {
    request.validate()?;

    if request.completed_at() > accepted_at {
        return Err(SubmissionValidationError::CompletionInFuture {
            attempt_id: request.attempt_id(),
        });
    }
    let expires_at = submission_expires_at(request);
    if accepted_at > expires_at {
        return Err(SubmissionValidationError::Expired {
            attempt_id: request.attempt_id(),
            expires_at,
        });
    }
    if already_submitted {
        return Err(SubmissionValidationError::Duplicate {
            attempt_id: request.attempt_id(),
        });
    }

    Ok(())
}

/// Process-local submission claim registry for deterministic domain tests and
/// adapters. Database-backed callers must still claim the attempt atomically.
#[derive(Debug, Default)]
pub struct BasicSubmissionRegistry {
    submitted_attempts: HashSet<Uuid>,
}

impl BasicSubmissionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Claims a submission once all shape, expiry, and duplicate checks pass.
    pub fn submit(
        &mut self,
        request: &BasicSettlementRequest,
        accepted_at: DateTime<Utc>,
    ) -> Result<(), SubmissionValidationError> {
        validate_submission(
            request,
            accepted_at,
            self.submitted_attempts.contains(&request.attempt_id()),
        )?;
        self.submitted_attempts.insert(request.attempt_id());
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, attempt_id: Uuid) -> bool {
        self.submitted_attempts.contains(&attempt_id)
    }
}

pub type SubmissionError = SubmissionValidationError;
pub type SubmissionRegistry = BasicSubmissionRegistry;

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::{
        eligible_option, submission_expires_at, validate_answer, validate_question,
        validate_submission, BasicSubmissionRegistry, McqValidationError,
        SubmissionValidationError,
    };
    use crate::quiz::basic::{
        IntendedQuestionSeconds, McqAnswer, McqOption, McqQuestion, SpecificationTopic,
    };
    use uuid::Uuid;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn question(options: Vec<McqOption>) -> McqQuestion {
        McqQuestion {
            id: id(1),
            topic: SpecificationTopic::from("syntax"),
            prompt: "Which option is correct?".to_owned(),
            options,
            intended_seconds: IntendedQuestionSeconds::new(15).expect("valid duration"),
        }
    }

    fn option(value: u128, is_correct: bool) -> McqOption {
        McqOption {
            id: id(value),
            text: format!("Option {value}"),
            is_correct,
        }
    }

    #[test]
    fn accepts_exactly_one_eligible_option() {
        let question = question(vec![option(2, true), option(3, false)]);

        assert_eq!(validate_question(&question), Ok(id(2)));
        assert_eq!(
            eligible_option(&question).expect("eligible option").id,
            id(2)
        );
    }

    #[test]
    fn rejects_zero_or_multiple_eligible_options() {
        let no_answer = question(vec![option(2, false)]);
        assert_eq!(
            validate_question(&no_answer),
            Err(McqValidationError::NoEligibleAnswer { question_id: id(1) })
        );

        let multiple_answers = question(vec![option(2, true), option(3, true)]);
        assert_eq!(
            validate_question(&multiple_answers),
            Err(McqValidationError::MultipleEligibleAnswers {
                question_id: id(1),
                count: 2,
            })
        );
    }

    #[test]
    fn answer_must_select_one_question_option() {
        let question = question(vec![option(2, true), option(3, false)]);
        assert!(validate_answer(
            &question,
            McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(3)),
            }
        )
        .is_ok());
        assert!(matches!(
            validate_answer(
                &question,
                McqAnswer {
                    question_id: id(1),
                    selected_option_id: None,
                }
            ),
            Err(McqValidationError::NoSelectedAnswer { .. })
        ));
        assert!(matches!(
            validate_answer(
                &question,
                McqAnswer {
                    question_id: id(1),
                    selected_option_id: Some(id(9)),
                }
            ),
            Err(McqValidationError::OptionNotFound { .. })
        ));
    }

    #[test]
    fn rejects_malformed_expired_and_duplicate_submissions() {
        let now = Utc::now();
        let valid_request = crate::quiz::basic::BasicSettlementRequest::for_attempt(
            id(10),
            id(20),
            vec![McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(2)),
            }],
            now,
            now,
        )
        .expect("valid request");
        let mut registry = BasicSubmissionRegistry::new();

        assert!(registry.submit(&valid_request, now).is_ok());
        assert!(matches!(
            registry.submit(&valid_request, now),
            Err(SubmissionValidationError::Duplicate { attempt_id }) if attempt_id == id(10)
        ));

        let expired_start = now - Duration::seconds(31);
        let expired = crate::quiz::basic::BasicSettlementRequest::for_attempt(
            id(11),
            id(20),
            vec![McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(2)),
            }],
            expired_start,
            now,
        )
        .expect("request shape is valid");
        assert!(matches!(
            validate_submission(&expired, now, false),
            Err(SubmissionValidationError::Expired { attempt_id, .. }) if attempt_id == id(11)
        ));
        assert_eq!(
            submission_expires_at(&valid_request),
            now + Duration::seconds(30)
        );

        assert!(
            serde_json::from_str::<crate::quiz::basic::BasicSettlementRequest>(&format!(
                r#"{{
                    "attempt_id": "{}",
                    "user_id": "{}",
                    "answers": [],
                    "started_at": "{}",
                    "completed_at": "{}"
                }}"#,
                id(12),
                id(20),
                now.to_rfc3339(),
                now.to_rfc3339(),
            ))
            .is_err()
        );
    }

    #[test]
    fn property_exactly_one_eligible_option_is_required() {
        for eligible_count in 0..=6 {
            let options = (0..6)
                .map(|value| option(value as u128 + 2, value < eligible_count))
                .collect();
            let question = question(options);
            let result = validate_question(&question);

            if eligible_count == 1 {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn property_expiry_boundary_scales_with_question_count() {
        let started_at = Utc::now();
        for count in 1..=8 {
            let answers = (0..count)
                .map(|value| McqAnswer {
                    question_id: id(value as u128 + 1),
                    selected_option_id: Some(id(value as u128 + 100)),
                })
                .collect();
            let request = crate::quiz::basic::BasicSettlementRequest::for_attempt(
                id(count as u128 + 10),
                id(20),
                answers,
                started_at,
                started_at,
            )
            .expect("valid request");
            let deadline = submission_expires_at(&request);

            assert!(validate_submission(&request, deadline, false).is_ok());
            assert!(matches!(
                validate_submission(&request, deadline + Duration::seconds(1), false),
                Err(SubmissionValidationError::Expired { .. })
            ));
        }
    }

    #[test]
    fn property_duplicate_claims_always_reject_after_first_success() {
        let now = Utc::now();
        let request = crate::quiz::basic::BasicSettlementRequest::for_attempt(
            id(10),
            id(20),
            vec![McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(2)),
            }],
            now,
            now,
        )
        .expect("valid request");
        let mut registry = BasicSubmissionRegistry::new();
        registry
            .submit(&request, now)
            .expect("first claim succeeds");

        for _ in 0..16 {
            assert!(matches!(
                registry.submit(&request, now),
                Err(SubmissionValidationError::Duplicate { .. })
            ));
        }
        assert!(registry.contains(id(10)));
    }
}
