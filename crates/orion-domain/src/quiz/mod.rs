//! Domain contracts for quiz features.
//!
//! The quiz domain is intentionally independent of persistence and transport
//! concerns. Database models can be converted into these types at the feature
//! boundary before scoring or validating a submission.

pub mod advanced;
pub mod basic;
#[cfg(test)]
#[path = "../elo.rs"]
pub mod elo;
#[cfg(not(test))]
pub use crate::elo;
pub mod scoring;
pub mod validation;

pub use advanced::{
    AdvancedActualValue, AdvancedAnswer, AdvancedLifecycleState, AdvancedPrediction,
    AdvancedQuestion, AdvancedValue, AdvancedValueSpec, AdvancedValueSpecError,
    MAX_ADVANCED_DECIMAL_SCALE,
};
pub use basic::{
    create_settlement_request, BasicAnswer, BasicOption, BasicQuestion, BasicResult,
    BasicSettlementInput, BasicSettlementRequest, BasicSettlementRequestError,
    IntendedQuestionSeconds, InvalidIntendedQuestionSeconds, McqAnswer, McqOption, McqQuestion,
    McqResult, SpecificationTopic, MAX_INTENDED_SECONDS, MIN_INTENDED_SECONDS,
};
pub use scoring::{
    advanced_elo_update, advanced_elo_update_with_source, advanced_error_pct, advanced_k_sa,
    advanced_prediction_elo_update, advanced_relative_error_pct, basic_mcq_elo_update,
    basic_mcq_elo_update_with_source, clamp_player_elo, clamp_question_elo, compute_elo,
    compute_elo_with_source, evaluate_answer, expected_score, mcq_elo_update,
    mcq_elo_update_with_source, score, score_answer, try_advanced_prediction_elo_update,
    AdvancedEloResult, BasicScore, EloResult, EloSourceMetadata, EloSourceMetadataError, Zone,
    BASIC_ELO_K, CORRECT_SCORE, ELO_POLICY_VERSION, INCORRECT_SCORE, PLAYER_ELO_MAX,
    PLAYER_ELO_MIN, QUESTION_ELO_MAX, QUESTION_ELO_MIN,
};
pub use validation::{
    advanced_lifecycle_state, eligible_option, submission_expires_at,
    validate_advanced_actual_value, validate_advanced_lifecycle, validate_advanced_prediction,
    validate_advanced_question, validate_answer, validate_question, validate_submission,
    AdvancedLifecycleTimestamps, AdvancedValidationError, BasicSubmissionRegistry,
    McqValidationError, SubmissionError, SubmissionRegistry, SubmissionValidationError,
    ValidationError, MAX_SUBMISSION_SECONDS_PER_QUESTION,
};
