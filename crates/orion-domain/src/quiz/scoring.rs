use serde::{Deserialize, Deserializer, Serialize};

use super::{
    basic::{McqAnswer, McqQuestion, McqResult},
    validation::{validate_answer, validate_question, McqValidationError},
};

pub const MIN_BASIC_REWARD: i32 = 5;
pub const MAX_BASIC_REWARD: i32 = 10;
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

/// An approved positive reward for one correct Basic Quiz answer.
///
/// Basic rewards are deliberately bounded so callers cannot inject arbitrary
/// point values into scoring or rating workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct BasicRewardInput(i32);

impl BasicRewardInput {
    /// Creates an approved Basic Quiz reward in the inclusive +5..=+10 range.
    pub const fn new(reward: i32) -> Result<Self, InvalidBasicRewardInput> {
        if reward < MIN_BASIC_REWARD || reward > MAX_BASIC_REWARD {
            return Err(InvalidBasicRewardInput { reward });
        }
        Ok(Self(reward))
    }

    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl TryFrom<i32> for BasicRewardInput {
    type Error = InvalidBasicRewardInput;

    fn try_from(reward: i32) -> Result<Self, Self::Error> {
        Self::new(reward)
    }
}

impl From<BasicRewardInput> for i32 {
    fn from(reward: BasicRewardInput) -> Self {
        reward.get()
    }
}

impl<'de> Deserialize<'de> for BasicRewardInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let reward = i32::deserialize(deserializer)?;
        Self::new(reward).map_err(|error| {
            serde::de::Error::custom(format!(
                "Basic reward must be between +{MIN_BASIC_REWARD} and +{MAX_BASIC_REWARD}, got {value}",
                value = error.reward
            ))
        })
    }
}

/// Returned when a Basic Quiz reward is outside the approved range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidBasicRewardInput {
    pub reward: i32,
}

/// Short alias for callers that do not need to emphasize the input boundary.
pub type BasicReward = BasicRewardInput;

/// A scored answer and the approved points awarded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicScore {
    pub result: McqResult,
    pub score: i32,
    pub reward: i32,
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

/// Calculates the Basic Quiz points for one validated answer.
pub fn score(
    question: &McqQuestion,
    answer: McqAnswer,
    reward: BasicRewardInput,
) -> Result<i32, McqValidationError> {
    let result = evaluate_answer(question, answer)?;
    Ok(if result.is_correct {
        reward.get()
    } else {
        INCORRECT_SCORE
    })
}

/// Evaluates an answer and returns both its result and awarded points.
pub fn score_answer(
    question: &McqQuestion,
    answer: McqAnswer,
    reward: BasicRewardInput,
) -> Result<BasicScore, McqValidationError> {
    let result = evaluate_answer(question, answer)?;
    let score = if result.is_correct {
        CORRECT_SCORE
    } else {
        INCORRECT_SCORE
    };
    let reward = if result.is_correct { reward.get() } else { 0 };
    Ok(BasicScore {
        result,
        score,
        reward,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        basic_mcq_elo_update, clamp_player_elo, clamp_question_elo, compute_elo, evaluate_answer,
        expected_score, mcq_elo_update, score, score_answer, BasicRewardInput, BASIC_ELO_K,
        CORRECT_SCORE, INCORRECT_SCORE, MAX_BASIC_REWARD, MIN_BASIC_REWARD, PLAYER_ELO_MAX,
        PLAYER_ELO_MIN, QUESTION_ELO_MAX, QUESTION_ELO_MIN,
    };
    use crate::quiz::basic::{
        IntendedQuestionSeconds, McqAnswer, McqOption, McqQuestion, SpecificationTopic,
    };
    use serde::Deserialize;
    use uuid::Uuid;

    #[test]
    fn accepts_approved_basic_reward_boundaries() {
        assert_eq!(BasicRewardInput::new(MIN_BASIC_REWARD).unwrap().get(), 5);
        assert_eq!(BasicRewardInput::new(MAX_BASIC_REWARD).unwrap().get(), 10);
    }

    #[test]
    fn rejects_rewards_outside_approved_range() {
        assert!(BasicRewardInput::new(MIN_BASIC_REWARD - 1).is_err());
        assert!(BasicRewardInput::new(MAX_BASIC_REWARD + 1).is_err());
        assert!(BasicRewardInput::new(-10).is_err());
    }

    #[test]
    fn reward_deserialization_preserves_approved_boundaries() {
        assert_eq!(
            serde_json::from_str::<BasicRewardInput>("5")
                .expect("minimum reward deserializes")
                .get(),
            MIN_BASIC_REWARD
        );
        assert_eq!(
            serde_json::from_str::<BasicRewardInput>("10")
                .expect("maximum reward deserializes")
                .get(),
            MAX_BASIC_REWARD
        );
        assert!(serde_json::from_str::<BasicRewardInput>("4").is_err());
        assert!(serde_json::from_str::<BasicRewardInput>("11").is_err());
    }

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
    fn score_awards_reward_only_for_the_eligible_answer() {
        let question = question();
        let reward = BasicRewardInput::new(7).expect("approved reward");
        let correct = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(2)),
        };
        let incorrect = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(3)),
        };

        assert_eq!(score(&question, correct, reward), Ok(7));
        assert_eq!(score(&question, incorrect, reward), Ok(0));
        assert!(
            evaluate_answer(&question, correct)
                .expect("valid answer")
                .is_correct
        );
        assert_eq!(
            score_answer(&question, correct, reward)
                .expect("scored answer")
                .score,
            CORRECT_SCORE
        );
        assert_eq!(
            score_answer(&question, correct, reward)
                .expect("scored answer")
                .reward,
            7
        );
    }

    #[test]
    fn property_score_is_bounded_and_monotonic_for_all_integer_inputs() {
        let question = question();
        let correct = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(2)),
        };
        let incorrect = McqAnswer {
            question_id: id(1),
            selected_option_id: Some(id(3)),
        };
        let mut previous = 0;

        for value in -20..=40 {
            match BasicRewardInput::new(value) {
                Ok(reward) => {
                    let correct_score = score(&question, correct, reward).expect("valid score");
                    let incorrect_score = score(&question, incorrect, reward).expect("valid score");
                    assert_eq!(correct_score, value);
                    assert_eq!(incorrect_score, INCORRECT_SCORE);
                    assert!((MIN_BASIC_REWARD..=MAX_BASIC_REWARD).contains(&correct_score));
                    assert!(correct_score >= previous);
                    previous = correct_score;
                }
                Err(_) => assert!(!(MIN_BASIC_REWARD..=MAX_BASIC_REWARD).contains(&value)),
            }
        }
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

        for reward_value in MIN_BASIC_REWARD..=MAX_BASIC_REWARD {
            let reward = BasicRewardInput::new(reward_value).expect("approved reward");
            for answer in answers {
                let expected = score_answer(&question, answer, reward).expect("valid score");
                for _ in 0..128 {
                    assert_eq!(
                        score_answer(&question, answer, reward).expect("valid score"),
                        expected
                    );
                }
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
        reward: i32,
        expected_correct: bool,
        expected_score: i32,
        expected_reward: i32,
    }

    #[derive(Deserialize)]
    struct GoldenFixture {
        question: McqQuestion,
        cases: Vec<GoldenCase>,
    }

    #[test]
    fn golden_fixtures_produce_approved_score_and_reward() {
        let fixture: GoldenFixture =
            serde_json::from_str(include_str!("fixtures/basic_scoring.json"))
                .expect("valid Basic Quiz scoring fixture");

        for case in fixture.cases {
            let answer = McqAnswer {
                question_id: fixture.question.id,
                selected_option_id: Some(case.selected_option_id),
            };
            let reward = BasicRewardInput::try_from(case.reward).expect("approved reward input");
            let scored = score_answer(&fixture.question, answer, reward)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));

            assert_eq!(
                scored.result.is_correct, case.expected_correct,
                "{}",
                case.name
            );
            assert_eq!(scored.score, case.expected_score, "{}", case.name);
            assert_eq!(scored.reward, case.expected_reward, "{}", case.name);
        }
    }
}
