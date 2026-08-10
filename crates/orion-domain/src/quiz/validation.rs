use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use uuid::Uuid;

use super::advanced::{
    AdvancedActualValue, AdvancedLifecycleState, AdvancedPrediction, AdvancedQuestion,
    AdvancedValueSpecError, MAX_ADVANCED_DECIMAL_SCALE,
};
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

/// Validation failures for Advanced value contracts, actual values, and
/// resolution timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdvancedValidationError {
    #[error("invalid Advanced value specification: {0}")]
    InvalidValueSpec(#[from] AdvancedValueSpecError),

    #[error("Advanced question prompt cannot be empty")]
    EmptyPrompt,

    #[error("Advanced market calendar ID cannot be empty")]
    EmptyCalendarId,

    #[error("Advanced market calendar version cannot be empty")]
    EmptyCalendarVersion,

    #[error("Advanced market timezone {timezone:?} is not a valid IANA-style identifier")]
    InvalidMarketTimezone { timezone: String },

    #[error("Advanced question expiry must be after its horizon")]
    ExpiryNotAfterHorizon,

    #[error("prediction targets question {actual_question_id}, expected {expected_question_id}")]
    QuestionMismatch {
        expected_question_id: Uuid,
        actual_question_id: Uuid,
    },

    #[error("Advanced prediction must be submitted before the question horizon")]
    PredictionAtOrAfterHorizon,

    #[error("{field} has decimal scale {scale}, but the question permits at most {max_scale}")]
    ValueScaleExceeded {
        field: &'static str,
        scale: u32,
        max_scale: u32,
    },

    #[error("Advanced actual value is not final")]
    ActualNotFinal,

    #[error("Advanced actual value was observed after the question horizon")]
    ActualObservedAfterHorizon,

    #[error("Advanced actual value was not available at or after the question horizon")]
    ActualAvailableBeforeHorizon,

    #[error("Advanced actual value became available after the question expiry")]
    ActualAvailableAfterExpiry,

    #[error("Advanced actual value became available before it was observed")]
    ActualAvailableBeforeObserved,

    #[error("Advanced actual-value source ID cannot be empty")]
    EmptyActualSourceId,

    #[error("Advanced actual-value source version cannot be empty")]
    EmptyActualSourceVersion,

    #[error("Advanced lifecycle timestamp {later} cannot be earlier than {earlier}")]
    InvalidTimestampOrder {
        earlier: &'static str,
        later: &'static str,
    },

    #[error("Advanced actual value is not available yet")]
    ActualNotAvailable,
}

/// The immutable timestamp set used to validate Advanced evaluation events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvancedLifecycleTimestamps {
    pub submitted_at: DateTime<Utc>,
    pub horizon_at: DateTime<Utc>,
    pub actual_observed_at: Option<DateTime<Utc>>,
    pub actual_available_at: Option<DateTime<Utc>>,
    pub scored_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub corrected_at: Option<DateTime<Utc>>,
}

fn is_iana_style_timezone(timezone: &str) -> bool {
    timezone == "UTC"
        || (timezone.contains('/')
            && !timezone.chars().any(char::is_whitespace)
            && !timezone.contains(".."))
}

fn validate_value_scale(
    value: rust_decimal::Decimal,
    field: &'static str,
    permitted_scale: u32,
) -> Result<(), AdvancedValidationError> {
    if value.scale() > permitted_scale {
        return Err(AdvancedValidationError::ValueScaleExceeded {
            field,
            scale: value.scale(),
            max_scale: permitted_scale,
        });
    }
    Ok(())
}

/// Validates immutable Advanced question configuration.
pub fn validate_advanced_question(
    question: &AdvancedQuestion,
) -> Result<(), AdvancedValidationError> {
    question.value_spec.validate()?;
    if question.prompt.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyPrompt);
    }
    if question.market_calendar_id.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyCalendarId);
    }
    if question.market_calendar_version.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyCalendarVersion);
    }
    if !is_iana_style_timezone(&question.market_timezone) {
        return Err(AdvancedValidationError::InvalidMarketTimezone {
            timezone: question.market_timezone.clone(),
        });
    }
    if question.expires_at <= question.horizon_at {
        return Err(AdvancedValidationError::ExpiryNotAfterHorizon);
    }
    Ok(())
}

/// Validates an Advanced prediction against its question and horizon.
pub fn validate_advanced_prediction(
    question: &AdvancedQuestion,
    prediction: &AdvancedPrediction,
) -> Result<(), AdvancedValidationError> {
    validate_advanced_question(question)?;
    if prediction.question_id != question.id {
        return Err(AdvancedValidationError::QuestionMismatch {
            expected_question_id: question.id,
            actual_question_id: prediction.question_id,
        });
    }
    validate_value_scale(
        prediction.value,
        "prediction",
        question.value_spec.scale.min(MAX_ADVANCED_DECIMAL_SCALE),
    )?;
    if prediction.submitted_at >= question.horizon_at {
        return Err(AdvancedValidationError::PredictionAtOrAfterHorizon);
    }
    Ok(())
}

/// Validates a source-backed actual value against its question's contract.
pub fn validate_advanced_actual_value(
    question: &AdvancedQuestion,
    actual: &AdvancedActualValue,
) -> Result<(), AdvancedValidationError> {
    validate_advanced_question(question)?;
    if actual.question_id != question.id {
        return Err(AdvancedValidationError::QuestionMismatch {
            expected_question_id: question.id,
            actual_question_id: actual.question_id,
        });
    }
    validate_value_scale(
        actual.value,
        "actual value",
        question.value_spec.scale.min(MAX_ADVANCED_DECIMAL_SCALE),
    )?;
    if !actual.is_final {
        return Err(AdvancedValidationError::ActualNotFinal);
    }
    if actual.observed_at > question.horizon_at {
        return Err(AdvancedValidationError::ActualObservedAfterHorizon);
    }
    if actual.available_at < question.horizon_at {
        return Err(AdvancedValidationError::ActualAvailableBeforeHorizon);
    }
    if actual.available_at > question.expires_at {
        return Err(AdvancedValidationError::ActualAvailableAfterExpiry);
    }
    if actual.available_at < actual.observed_at {
        return Err(AdvancedValidationError::ActualAvailableBeforeObserved);
    }
    if actual.source_id.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyActualSourceId);
    }
    if actual.source_version.trim().is_empty() {
        return Err(AdvancedValidationError::EmptyActualSourceVersion);
    }
    Ok(())
}

/// Validates the timestamp ordering for an Advanced evaluation lifecycle.
pub fn validate_advanced_lifecycle(
    timestamps: AdvancedLifecycleTimestamps,
) -> Result<(), AdvancedValidationError> {
    if timestamps.submitted_at >= timestamps.horizon_at {
        return Err(AdvancedValidationError::InvalidTimestampOrder {
            earlier: "submitted_at",
            later: "horizon_at",
        });
    }
    if let Some(actual_observed_at) = timestamps.actual_observed_at {
        if actual_observed_at > timestamps.horizon_at {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "actual_observed_at",
                later: "horizon_at",
            });
        }
    }
    if let Some(actual_available_at) = timestamps.actual_available_at {
        if actual_available_at < timestamps.horizon_at {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "horizon_at",
                later: "actual_available_at",
            });
        }
        if let Some(actual_observed_at) = timestamps.actual_observed_at {
            if actual_available_at < actual_observed_at {
                return Err(AdvancedValidationError::InvalidTimestampOrder {
                    earlier: "actual_observed_at",
                    later: "actual_available_at",
                });
            }
        }
    }
    if let Some(scored_at) = timestamps.scored_at {
        let Some(actual_available_at) = timestamps.actual_available_at else {
            return Err(AdvancedValidationError::ActualNotAvailable);
        };
        if scored_at < actual_available_at {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "actual_available_at",
                later: "scored_at",
            });
        }
    }
    if let Some(settled_at) = timestamps.settled_at {
        let Some(scored_at) = timestamps.scored_at else {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "scored_at",
                later: "settled_at",
            });
        };
        if settled_at < scored_at {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "scored_at",
                later: "settled_at",
            });
        }
    }
    if let Some(corrected_at) = timestamps.corrected_at {
        let Some(settled_at) = timestamps.settled_at else {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "settled_at",
                later: "corrected_at",
            });
        };
        if corrected_at < settled_at {
            return Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "settled_at",
                later: "corrected_at",
            });
        }
    }
    Ok(())
}

/// Determines the next valid Advanced lifecycle state at a server timestamp.
pub fn advanced_lifecycle_state(
    question: &AdvancedQuestion,
    prediction: &AdvancedPrediction,
    actual: Option<&AdvancedActualValue>,
    now: DateTime<Utc>,
) -> Result<AdvancedLifecycleState, AdvancedValidationError> {
    validate_advanced_prediction(question, prediction)?;

    if let Some(actual) = actual {
        validate_advanced_actual_value(question, actual)?;
        if actual.available_at <= now {
            return Ok(AdvancedLifecycleState::ActualAvailable);
        }
        return Ok(if now < question.horizon_at {
            AdvancedLifecycleState::Pending
        } else {
            AdvancedLifecycleState::Delayed
        });
    }

    if now <= prediction.submitted_at {
        return Ok(AdvancedLifecycleState::Submitted);
    }
    if now < question.horizon_at {
        return Ok(AdvancedLifecycleState::Pending);
    }
    if now < question.expires_at {
        Ok(AdvancedLifecycleState::Delayed)
    } else {
        Ok(AdvancedLifecycleState::Expired)
    }
}

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
    use rust_decimal::Decimal;

    use super::{
        advanced_lifecycle_state, eligible_option, submission_expires_at,
        validate_advanced_lifecycle, validate_advanced_prediction, validate_advanced_question,
        validate_answer, validate_question, validate_submission, AdvancedLifecycleTimestamps,
        AdvancedValidationError, BasicSubmissionRegistry, McqValidationError,
        SubmissionValidationError,
    };
    use crate::quiz::advanced::{
        AdvancedActualValue, AdvancedLifecycleState, AdvancedPrediction, AdvancedQuestion,
        AdvancedValueSpec,
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

    fn advanced_question(horizon: chrono::DateTime<Utc>) -> AdvancedQuestion {
        AdvancedQuestion {
            id: id(1),
            topic: SpecificationTopic::from("markets"),
            prompt: "What is the value at the horizon?".to_owned(),
            value_spec: AdvancedValueSpec::new("price", Some("USD".to_owned()), 2)
                .expect("valid value spec"),
            market_calendar_id: "NYSE".to_owned(),
            market_calendar_version: "2026.1".to_owned(),
            market_timezone: "America/New_York".to_owned(),
            horizon_at: horizon,
            expires_at: horizon + Duration::hours(24),
        }
    }

    fn advanced_prediction(
        question_id: Uuid,
        submitted_at: chrono::DateTime<Utc>,
    ) -> AdvancedPrediction {
        AdvancedPrediction::new(question_id, Decimal::new(12345, 2), submitted_at)
    }

    fn advanced_actual(
        question_id: Uuid,
        horizon: chrono::DateTime<Utc>,
        available_at: chrono::DateTime<Utc>,
    ) -> AdvancedActualValue {
        AdvancedActualValue::new(
            question_id,
            Decimal::new(12350, 2),
            horizon,
            available_at,
            "market-provider".to_owned(),
            "2026-08-10".to_owned(),
            true,
        )
    }

    #[test]
    fn advanced_validation_enforces_submission_and_actual_boundaries() {
        let horizon = Utc::now() + Duration::hours(1);
        let question = advanced_question(horizon);
        let prediction = advanced_prediction(question.id, horizon - Duration::seconds(1));
        assert!(validate_advanced_question(&question).is_ok());
        assert!(validate_advanced_prediction(&question, &prediction).is_ok());

        let at_horizon = advanced_prediction(question.id, horizon);
        assert!(matches!(
            validate_advanced_prediction(&question, &at_horizon),
            Err(AdvancedValidationError::PredictionAtOrAfterHorizon)
        ));

        let before_horizon = advanced_actual(question.id, horizon, horizon - Duration::seconds(1));
        assert!(matches!(
            super::validate_advanced_actual_value(&question, &before_horizon),
            Err(AdvancedValidationError::ActualAvailableBeforeHorizon)
        ));
    }

    #[test]
    fn advanced_lifecycle_distinguishes_pending_delayed_expired_and_available() {
        let horizon = Utc::now() + Duration::hours(1);
        let question = advanced_question(horizon);
        let prediction = advanced_prediction(question.id, horizon - Duration::minutes(1));

        assert_eq!(
            advanced_lifecycle_state(
                &question,
                &prediction,
                None,
                horizon - Duration::seconds(30)
            )
            .expect("pending state"),
            AdvancedLifecycleState::Pending
        );
        assert_eq!(
            advanced_lifecycle_state(&question, &prediction, None, horizon + Duration::hours(1))
                .expect("delayed state"),
            AdvancedLifecycleState::Delayed
        );
        assert_eq!(
            advanced_lifecycle_state(&question, &prediction, None, question.expires_at)
                .expect("expired state"),
            AdvancedLifecycleState::Expired
        );

        let actual = advanced_actual(question.id, horizon, horizon + Duration::minutes(1));
        assert_eq!(
            advanced_lifecycle_state(
                &question,
                &prediction,
                Some(&actual),
                horizon + Duration::minutes(2)
            )
            .expect("actual available state"),
            AdvancedLifecycleState::ActualAvailable
        );
    }

    #[test]
    fn advanced_lifecycle_timestamp_order_is_monotonic() {
        let now = Utc::now();
        let valid = AdvancedLifecycleTimestamps {
            submitted_at: now,
            horizon_at: now + Duration::minutes(1),
            actual_observed_at: Some(now + Duration::minutes(1)),
            actual_available_at: Some(now + Duration::minutes(2)),
            scored_at: Some(now + Duration::minutes(3)),
            settled_at: Some(now + Duration::minutes(4)),
            corrected_at: Some(now + Duration::minutes(5)),
        };
        assert!(validate_advanced_lifecycle(valid).is_ok());

        let mut invalid = valid;
        invalid.settled_at = Some(now + Duration::minutes(2));
        assert!(matches!(
            validate_advanced_lifecycle(invalid),
            Err(AdvancedValidationError::InvalidTimestampOrder {
                earlier: "scored_at",
                later: "settled_at"
            })
        ));
    }
}
