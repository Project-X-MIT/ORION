use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use super::{
    advanced::{AdvancedActualValue, AdvancedPrediction},
    basic::{McqAnswer, McqQuestion, McqResult},
    validation::{validate_answer, validate_question, McqValidationError},
};

pub const INCORRECT_SCORE: i32 = 0;
pub const CORRECT_SCORE: i32 = 1;

/// Minimum player rating accepted by the shared Basic ELO calculation.
pub const PLAYER_ELO_MIN: f64 = 100.0;
/// Maximum player rating accepted by the shared Basic ELO calculation.
pub const PLAYER_ELO_MAX: f64 = 3000.0;
/// Minimum question rating accepted by the shared Basic ELO calculation.
pub const QUESTION_ELO_MIN: f64 = 100.0;
/// Maximum question rating accepted by the shared Basic ELO calculation.
pub const QUESTION_ELO_MAX: f64 = 2400.0;
/// Fixed K-factor approved for Basic Quiz MCQ rating updates.
pub const BASIC_ELO_K: f64 = 32.0;

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.max(min).min(max)
    }
}

fn normalize_k(k: f64) -> f64 {
    if k.is_finite() && k > 0.0 {
        k
    } else {
        0.0
    }
}

fn normalize_actual_score(score: f64) -> f64 {
    if score.is_finite() {
        clamp(score, 0.0, 1.0)
    } else {
        0.0
    }
}

/// Clamps a player rating to the approved `[100, 3000]` range.
#[must_use]
pub fn clamp_player_elo(elo: f64) -> f64 {
    clamp(elo, PLAYER_ELO_MIN, PLAYER_ELO_MAX)
}

/// Clamps a question rating to the approved `[100, 2400]` range.
#[must_use]
pub fn clamp_question_elo(elo: f64) -> f64 {
    clamp(elo, QUESTION_ELO_MIN, QUESTION_ELO_MAX)
}

/// The result of one deterministic player/question ELO update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EloResult {
    /// Raw formula delta: `K * (Sa - Ea)`.
    pub point_delta: f64,
    /// Expected score before applying the answer outcome.
    pub expected_score: f64,
    /// Player rating after applying and clamping the update.
    pub player_new_elo: f64,
    /// Question rating after applying the inverse update and clamping it.
    pub question_new_elo: f64,
}

impl EloResult {
    /// Returns the actually applied player change after input/output guardrails.
    #[must_use]
    pub fn applied_player_delta(&self, original_player_elo: f64) -> f64 {
        self.player_new_elo - clamp_player_elo(original_player_elo)
    }

    /// Returns the actually applied question change after input/output guardrails.
    #[must_use]
    pub fn applied_question_delta(&self, original_question_elo: f64) -> f64 {
        self.question_new_elo - clamp_question_elo(original_question_elo)
    }
}

/// Computes the player's expected score against a question.
///
/// Both ratings are clamped before they participate in the formula:
/// `Ea = 1 / (1 + 10^((Rb - Ra) / 400))`.
#[must_use]
pub fn expected_score(player_elo: f64, question_elo: f64) -> f64 {
    let player_elo = clamp_player_elo(player_elo);
    let question_elo = clamp_question_elo(question_elo);
    1.0 / (1.0 + 10_f64.powf((question_elo - player_elo) / 400.0))
}

/// Applies the shared ELO update formula to a player/question pair.
///
/// `k` is normalized to zero for non-positive or non-finite input. `sa` is
/// normalized to the inclusive `[0, 1]` range. Input ratings and final ratings
/// are clamped with their respective player/question guardrails.
#[must_use]
pub fn compute_elo(player_elo: f64, question_elo: f64, k: f64, sa: f64) -> EloResult {
    let player_elo = clamp_player_elo(player_elo);
    let question_elo = clamp_question_elo(question_elo);
    let k = normalize_k(k);
    let sa = normalize_actual_score(sa);
    let expected_score = expected_score(player_elo, question_elo);
    let point_delta = k * (sa - expected_score);

    EloResult {
        point_delta,
        expected_score,
        player_new_elo: clamp_player_elo(player_elo + point_delta),
        question_new_elo: clamp_question_elo(question_elo - point_delta),
    }
}

/// Calculates the shared ELO update for a Basic MCQ answer.
#[must_use]
pub fn mcq_elo_update(player_elo: f64, question_elo: f64, k: f64, correct: bool) -> EloResult {
    compute_elo(player_elo, question_elo, k, if correct { 1.0 } else { 0.0 })
}

/// Calculates a Basic Quiz MCQ update using the approved fixed `K = 32`.
#[must_use]
pub fn basic_mcq_elo_update(player_elo: f64, question_elo: f64, correct: bool) -> EloResult {
    mcq_elo_update(player_elo, question_elo, BASIC_ELO_K, correct)
}

/// A Basic answer outcome. Rating movement is calculated separately by
/// [`basic_mcq_elo_update`] using the fixed Basic K-factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicScore {
    pub result: McqResult,
    pub score: i32,
}

/// Evaluates one answer against a question's unique eligible option.
pub fn evaluate_answer(
    question: &McqQuestion,
    answer: McqAnswer,
) -> Result<McqResult, McqValidationError> {
    let correct_option_id = validate_question(question)?;
    validate_answer(question, answer)?;
    let is_correct = answer.selected_option_id == Some(correct_option_id);

    Ok(McqResult {
        question_id: question.id,
        selected_option_id: answer.selected_option_id,
        correct_option_id,
        is_correct,
    })
}

/// Calculates the Basic Quiz outcome (`1` for correct, `0` for incorrect).
pub fn score(question: &McqQuestion, answer: McqAnswer) -> Result<i32, McqValidationError> {
    let result = evaluate_answer(question, answer)?;
    Ok(if result.is_correct {
        CORRECT_SCORE
    } else {
        INCORRECT_SCORE
    })
}

/// Evaluates an answer and returns its binary Basic outcome.
pub fn score_answer(
    question: &McqQuestion,
    answer: McqAnswer,
) -> Result<BasicScore, McqValidationError> {
    let result = evaluate_answer(question, answer)?;
    let score = if result.is_correct {
        CORRECT_SCORE
    } else {
        INCORRECT_SCORE
    };
    Ok(BasicScore { result, score })
}

/// Zone classification for an Advanced prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Zone {
    /// Error 0-8%: a correct prediction with a decreasing reward K.
    Win,
    /// Error 9-10%: no rating movement.
    Neutral,
    /// Error 11-50%: an increasing penalty K.
    MildPenalty,
    /// Error 51% or more: the maximum penalty K.
    SeverePenalty,
}

impl std::fmt::Display for Zone {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Win => write!(formatter, "Zone 1 (Win)"),
            Self::Neutral => write!(formatter, "Zone 0 (Neutral)"),
            Self::MildPenalty => write!(formatter, "Zone 2 (Mild Penalty)"),
            Self::SeverePenalty => write!(formatter, "Zone 3 (Severe Penalty)"),
        }
    }
}

/// Returns the Advanced `(zone, K, Sa)` tuple for a rounded error percentage.
///
/// The table is deliberately discrete and deterministic. Values above 50%
/// all use the severe-penalty bucket, so callers do not need to cap the raw
/// relative error before selecting a zone.
#[must_use]
pub fn advanced_k_sa(error_pct: u32) -> (Zone, f64, f64) {
    match error_pct {
        0 => (Zone::Win, 45.0, 1.0),
        1 => (Zone::Win, 42.75, 1.0),
        2..=3 => (Zone::Win, 40.50, 1.0),
        4..=5 => (Zone::Win, 38.25, 1.0),
        6..=8 => (Zone::Win, 36.0, 1.0),
        9..=10 => (Zone::Neutral, 0.0, 0.0),
        11..=20 => (Zone::MildPenalty, 15.0, 0.0),
        21..=30 => (Zone::MildPenalty, 25.0, 0.0),
        31..=50 => (Zone::MildPenalty, 30.0, 0.0),
        _ => (Zone::SeverePenalty, 45.0, 0.0),
    }
}

/// Computes the exact non-negative relative error percentage.
///
/// For a zero actual value, zero prediction is exact and any other prediction
/// is assigned 100% error, matching the Advanced evaluation specification.
#[must_use]
pub fn advanced_relative_error_pct(predicted: Decimal, actual: Decimal) -> Decimal {
    if actual.is_zero() {
        if predicted.is_zero() {
            Decimal::ZERO
        } else {
            Decimal::from(100)
        }
    } else {
        ((predicted - actual).abs() / actual.abs()) * Decimal::from(100)
    }
}

/// Rounds a relative error to the integer used by [`advanced_k_sa`].
///
/// Decimal rounding uses half-away-from-zero. Relative errors are
/// non-negative, so `4.5` becomes `5`. Values that exceed `u32` are safely
/// represented as `u32::MAX`, which is already in the severe-penalty bucket.
#[must_use]
pub fn advanced_error_pct(predicted: Decimal, actual: Decimal) -> u32 {
    advanced_relative_error_pct(predicted, actual)
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_u32()
        .unwrap_or(u32::MAX)
}

/// The complete result of an Advanced ELO update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdvancedEloResult {
    /// Shared guarded ELO result.
    pub elo: EloResult,
    /// Exact relative error before zone rounding.
    #[serde(with = "rust_decimal::serde::str")]
    pub relative_error_pct: Decimal,
    /// Integer error percentage used by the zone lookup.
    pub error_pct: u32,
    /// Zone selected from the rounded error percentage.
    pub zone: Zone,
    /// K selected from the zone table.
    pub k: f64,
    /// Sa selected from the zone table.
    pub sa: f64,
}

impl std::fmt::Display for AdvancedEloResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "  Relative Error     : {}%\n  Rounded Error      : {}%\n  Zone               : {}\n  K                  : {:.2}\n  Sa                 : {}\n  Expected Score (Ea): {:.6}\n  Raw Point Delta    : {:+.4}\n  Player New ELO     : {:.2}\n  Question New ELO   : {:.2}",
            self.relative_error_pct,
            self.error_pct,
            self.zone,
            self.k,
            self.sa,
            self.elo.expected_score,
            self.elo.point_delta,
            self.elo.player_new_elo,
            self.elo.question_new_elo,
        )
    }
}

/// Calculates the Advanced ELO update from a prediction and its actual value.
///
/// The relative error is calculated with exact decimals, rounded only for the
/// zone lookup, and then passed to the shared guarded ELO formula.
#[must_use]
pub fn advanced_elo_update(
    player_elo: f64,
    question_elo: f64,
    predicted: Decimal,
    actual: Decimal,
) -> AdvancedEloResult {
    let relative_error_pct = advanced_relative_error_pct(predicted, actual);
    let error_pct = advanced_error_pct(predicted, actual);
    let (zone, k, sa) = advanced_k_sa(error_pct);
    let elo = compute_elo(player_elo, question_elo, k, sa);

    AdvancedEloResult {
        elo,
        relative_error_pct,
        error_pct,
        zone,
        k,
        sa,
    }
}

/// Calculates Advanced ELO directly from validated domain values.
#[must_use]
pub fn advanced_prediction_elo_update(
    player_elo: f64,
    question_elo: f64,
    prediction: &AdvancedPrediction,
    actual: &AdvancedActualValue,
) -> AdvancedEloResult {
    advanced_elo_update(player_elo, question_elo, prediction.value, actual.value)
}

#[cfg(test)]
mod tests {
    use super::{
        advanced_elo_update, advanced_error_pct, advanced_k_sa, advanced_relative_error_pct,
        basic_mcq_elo_update, clamp_player_elo, clamp_question_elo, compute_elo, evaluate_answer,
        expected_score, mcq_elo_update, score, score_answer, Zone, BASIC_ELO_K, CORRECT_SCORE,
        INCORRECT_SCORE, PLAYER_ELO_MAX, PLAYER_ELO_MIN, QUESTION_ELO_MAX, QUESTION_ELO_MIN,
    };
    use crate::quiz::basic::{
        IntendedQuestionSeconds, McqAnswer, McqOption, McqQuestion, SpecificationTopic,
    };
    use rust_decimal::Decimal;
    use serde::Deserialize;
    use uuid::Uuid;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn question() -> McqQuestion {
        McqQuestion {
            id: id(1),
            topic: SpecificationTopic::from("syntax"),
            prompt: "Which option is correct?".to_owned(),
            options: vec![
                McqOption {
                    id: id(2),
                    text: "Correct".to_owned(),
                    is_correct: true,
                },
                McqOption {
                    id: id(3),
                    text: "Incorrect".to_owned(),
                    is_correct: false,
                },
            ],
            intended_seconds: IntendedQuestionSeconds::new(15).expect("valid duration"),
        }
    }

    #[test]
    fn score_returns_binary_outcome_for_the_eligible_answer() {
        let question = question();
        let correct = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(2)),
        };
        let incorrect = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(3)),
        };

        assert_eq!(score(&question, correct), Ok(CORRECT_SCORE));
        assert_eq!(score(&question, incorrect), Ok(INCORRECT_SCORE));
        assert!(
            evaluate_answer(&question, correct)
                .expect("valid answer")
                .is_correct
        );
        assert_eq!(
            score_answer(&question, correct)
                .expect("scored answer")
                .score,
            CORRECT_SCORE
        );
    }

    #[test]
    fn score_is_always_zero_or_one() {
        let question = question();
        let correct = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(2)),
        };
        let incorrect = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(3)),
        };
        assert!([INCORRECT_SCORE, CORRECT_SCORE].contains(&score(&question, correct).unwrap()));
        assert!([INCORRECT_SCORE, CORRECT_SCORE].contains(&score(&question, incorrect).unwrap()));
    }

    #[test]
    fn scoring_is_deterministic_across_repeated_runs() {
        let question = question();
        let answers = [
            McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(2)),
            },
            McqAnswer {
                question_id: id(1),
                selected_option_id: Some(id(3)),
            },
        ];

        for answer in answers {
            let expected = score_answer(&question, answer).expect("valid score");
            for _ in 0..128 {
                assert_eq!(
                    score_answer(&question, answer).expect("valid score"),
                    expected
                );
            }
        }
    }

    const EPSILON: f64 = 1e-9;

    fn approx_eq(left: f64, right: f64) -> bool {
        (left - right).abs() < EPSILON
    }

    #[test]
    fn expected_score_follows_rating_order() {
        assert!(approx_eq(expected_score(1500.0, 1500.0), 0.5));
        assert!(expected_score(1800.0, 1500.0) > 0.5);
        assert!(expected_score(1200.0, 1500.0) < 0.5);
    }

    #[test]
    fn mcq_elo_update_uses_correct_and_incorrect_outcomes() {
        let correct = mcq_elo_update(1500.0, 1500.0, 32.0, true);
        let incorrect = mcq_elo_update(1500.0, 1500.0, 32.0, false);

        assert!(approx_eq(correct.point_delta, 16.0));
        assert!(approx_eq(incorrect.point_delta, -16.0));
        assert!(correct.player_new_elo > 1500.0);
        assert!(correct.question_new_elo < 1500.0);
        assert!(incorrect.player_new_elo < 1500.0);
        assert!(incorrect.question_new_elo > 1500.0);
    }

    #[test]
    fn basic_mcq_uses_the_approved_k_factor() {
        assert_eq!(BASIC_ELO_K, 32.0);
        assert_eq!(
            basic_mcq_elo_update(1500.0, 1500.0, true),
            mcq_elo_update(1500.0, 1500.0, BASIC_ELO_K, true)
        );
        assert!(approx_eq(
            basic_mcq_elo_update(1500.0, 1500.0, true).point_delta,
            BASIC_ELO_K / 2.0
        ));
    }

    #[test]
    fn elo_update_is_zero_sum_away_from_guardrails() {
        let result = mcq_elo_update(1400.0, 1600.0, 24.0, true);
        let total_change = (result.player_new_elo - 1400.0) + (result.question_new_elo - 1600.0);

        assert!(approx_eq(total_change, 0.0));
    }

    #[test]
    fn elo_inputs_are_clamped_before_calculation() {
        let low_player = compute_elo(50.0, 1500.0, 32.0, 1.0);
        let min_player = compute_elo(PLAYER_ELO_MIN, 1500.0, 32.0, 1.0);
        let high_player = compute_elo(3500.0, 1500.0, 32.0, 1.0);
        let max_player = compute_elo(PLAYER_ELO_MAX, 1500.0, 32.0, 1.0);
        let low_question = compute_elo(1500.0, 50.0, 32.0, 1.0);
        let min_question = compute_elo(1500.0, QUESTION_ELO_MIN, 32.0, 1.0);
        let high_question = compute_elo(1500.0, 2800.0, 32.0, 1.0);
        let max_question = compute_elo(1500.0, QUESTION_ELO_MAX, 32.0, 1.0);

        assert_eq!(low_player, min_player);
        assert_eq!(high_player, max_player);
        assert_eq!(low_question, min_question);
        assert_eq!(high_question, max_question);
        assert_eq!(clamp_player_elo(f64::NAN), PLAYER_ELO_MIN);
        assert_eq!(clamp_question_elo(f64::NAN), QUESTION_ELO_MIN);
    }

    #[test]
    fn elo_outputs_never_cross_player_or_question_boundaries() {
        let cases = [
            (PLAYER_ELO_MAX - 1.0, QUESTION_ELO_MAX, true),
            (PLAYER_ELO_MIN + 1.0, QUESTION_ELO_MIN, false),
            (PLAYER_ELO_MAX, QUESTION_ELO_MAX - 1.0, false),
            (PLAYER_ELO_MIN, QUESTION_ELO_MIN + 1.0, true),
        ];

        for (player_elo, question_elo, correct) in cases {
            let result = mcq_elo_update(player_elo, question_elo, 45.0, correct);
            assert!((PLAYER_ELO_MIN..=PLAYER_ELO_MAX).contains(&result.player_new_elo));
            assert!((QUESTION_ELO_MIN..=QUESTION_ELO_MAX).contains(&result.question_new_elo));
        }
    }

    #[test]
    fn malformed_elo_inputs_cannot_reverse_or_poison_an_update() {
        for k in [-10.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = compute_elo(1500.0, 1500.0, k, 1.0);
            assert!(approx_eq(result.point_delta, 0.0));
            assert!(approx_eq(result.player_new_elo, 1500.0));
            assert!(approx_eq(result.question_new_elo, 1500.0));
        }

        let above_one = compute_elo(1500.0, 1500.0, 32.0, 2.0);
        let one = compute_elo(1500.0, 1500.0, 32.0, 1.0);
        let below_zero = compute_elo(1500.0, 1500.0, 32.0, -1.0);
        let zero = compute_elo(1500.0, 1500.0, 32.0, 0.0);
        let not_a_score = compute_elo(1500.0, 1500.0, 32.0, f64::NAN);

        assert_eq!(above_one, one);
        assert_eq!(below_zero, zero);
        assert_eq!(not_a_score, zero);
    }

    #[test]
    fn elo_calculation_is_deterministic_across_repeated_runs() {
        for player_elo in [100.0, 500.0, 1500.0, 2999.0, 3000.0] {
            for question_elo in [100.0, 800.0, 1500.0, 2399.0, 2400.0] {
                for correct in [false, true] {
                    let expected = mcq_elo_update(player_elo, question_elo, 32.0, correct);
                    for _ in 0..128 {
                        assert_eq!(
                            mcq_elo_update(player_elo, question_elo, 32.0, correct),
                            expected
                        );
                    }
                }
            }
        }
    }

    #[derive(Deserialize)]
    struct GoldenCase {
        name: String,
        selected_option_id: Uuid,
        expected_correct: bool,
        expected_score: i32,
    }

    #[derive(Deserialize)]
    struct GoldenFixture {
        question: McqQuestion,
        cases: Vec<GoldenCase>,
    }

    #[test]
    fn golden_fixtures_produce_approved_binary_score() {
        let fixture: GoldenFixture =
            serde_json::from_str(include_str!("fixtures/basic_scoring.json"))
                .expect("valid Basic Quiz scoring fixture");

        for case in fixture.cases {
            let answer = McqAnswer {
                question_id: fixture.question.id,
                selected_option_id: Some(case.selected_option_id),
            };
            let scored = score_answer(&fixture.question, answer)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));

            assert_eq!(
                scored.result.is_correct, case.expected_correct,
                "{}",
                case.name
            );
            assert_eq!(scored.score, case.expected_score, "{}", case.name);
        }
    }

    #[test]
    fn advanced_zone_table_matches_all_boundary_buckets() {
        let cases = [
            (0, Zone::Win, 45.0, 1.0),
            (1, Zone::Win, 42.75, 1.0),
            (2, Zone::Win, 40.50, 1.0),
            (4, Zone::Win, 38.25, 1.0),
            (6, Zone::Win, 36.0, 1.0),
            (9, Zone::Neutral, 0.0, 0.0),
            (11, Zone::MildPenalty, 15.0, 0.0),
            (21, Zone::MildPenalty, 25.0, 0.0),
            (31, Zone::MildPenalty, 30.0, 0.0),
            (51, Zone::SeverePenalty, 45.0, 0.0),
        ];

        for (error_pct, expected_zone, expected_k, expected_sa) in cases {
            let (zone, k, sa) = advanced_k_sa(error_pct);
            assert_eq!(zone, expected_zone, "error_pct={error_pct}");
            assert_eq!(k, expected_k, "error_pct={error_pct}");
            assert_eq!(sa, expected_sa, "error_pct={error_pct}");
        }
    }

    #[test]
    fn advanced_error_uses_exact_decimal_math_and_half_away_rounding() {
        let repeating_error = advanced_relative_error_pct(Decimal::new(5, 7), Decimal::new(9, 7));
        assert!(repeating_error > Decimal::from(44));
        assert!(repeating_error < Decimal::from(45));
        assert_eq!(
            advanced_error_pct(Decimal::new(5, 7), Decimal::new(9, 7)),
            44
        );
        assert_eq!(advanced_error_pct(Decimal::from(5), Decimal::from(5)), 0);
        assert_eq!(
            advanced_error_pct(Decimal::new(5225, 3), Decimal::from(5)),
            5
        );
        assert_eq!(advanced_error_pct(Decimal::ZERO, Decimal::ZERO), 0);
        assert_eq!(advanced_error_pct(Decimal::from(1), Decimal::ZERO), 100);
    }

    #[test]
    fn advanced_elo_uses_zone_outcome_and_shared_guardrails() {
        let win = advanced_elo_update(1400.0, 1600.0, Decimal::from(5), Decimal::from(5));
        assert_eq!(win.zone, Zone::Win);
        assert!(win.elo.point_delta > 0.0);

        let neutral = advanced_elo_update(1400.0, 1600.0, Decimal::new(545, 2), Decimal::from(5));
        assert_eq!(neutral.zone, Zone::Neutral);
        assert_eq!(neutral.elo.point_delta, 0.0);

        let severe = advanced_elo_update(1400.0, 1600.0, Decimal::from(8), Decimal::from(5));
        assert_eq!(severe.zone, Zone::SeverePenalty);
        assert!(severe.elo.point_delta < 0.0);

        let bounded = advanced_elo_update(3000.0, 2399.0, Decimal::from(8), Decimal::from(5));
        assert!(bounded.elo.player_new_elo < PLAYER_ELO_MAX);
        assert_eq!(bounded.elo.question_new_elo, QUESTION_ELO_MAX);
    }
}
