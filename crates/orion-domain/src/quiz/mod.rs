//! Domain contracts for quiz features.
//!
//! The quiz domain is intentionally independent of persistence and transport
//! concerns. Database models can be converted into these types at the feature
//! boundary before scoring or validating a submission.

pub mod basic;
pub mod scoring;
pub mod validation;

pub use basic::{
    create_settlement_request, BasicAnswer, BasicOption, BasicQuestion, BasicResult,
    BasicSettlementInput, BasicSettlementRequest, BasicSettlementRequestError,
    IntendedQuestionSeconds, InvalidIntendedQuestionSeconds, McqAnswer, McqOption, McqQuestion,
    McqResult, SpecificationTopic, MAX_INTENDED_SECONDS, MIN_INTENDED_SECONDS,
};
pub use scoring::{
    basic_mcq_elo_update, clamp_player_elo, clamp_question_elo, compute_elo, evaluate_answer,
    expected_score, mcq_elo_update, score, score_answer, BasicReward, BasicRewardInput, BasicScore,
    EloResult, InvalidBasicRewardInput, BASIC_ELO_K, CORRECT_SCORE, INCORRECT_SCORE,
    MAX_BASIC_REWARD, MIN_BASIC_REWARD, PLAYER_ELO_MAX, PLAYER_ELO_MIN, QUESTION_ELO_MAX,
    QUESTION_ELO_MIN,
};
pub use validation::{
    eligible_option, submission_expires_at, validate_answer, validate_question,
    validate_submission, BasicSubmissionRegistry, McqValidationError, SubmissionError,
    SubmissionRegistry, SubmissionValidationError, ValidationError,
    MAX_SUBMISSION_SECONDS_PER_QUESTION,
};
