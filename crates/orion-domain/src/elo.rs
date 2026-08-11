use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// Minimum player rating accepted by the shared Elo calculation.
pub const PLAYER_ELO_MIN: f64 = 100.0;
/// Maximum player rating accepted by the shared Elo calculation.
pub const PLAYER_ELO_MAX: f64 = 3000.0;
/// Minimum question rating accepted by the shared Elo calculation.
pub const QUESTION_ELO_MIN: f64 = 100.0;
/// Maximum question rating accepted by the shared Elo calculation.
pub const QUESTION_ELO_MAX: f64 = 2400.0;
/// Fixed K-factor approved for Basic Quiz MCQ rating updates.
pub const BASIC_ELO_K: f64 = 20.0;
/// Version of the approved shared Elo policy that produced an output.
pub const ELO_POLICY_VERSION: u16 = 1;

/// Provenance required for every Elo result.
///
/// The constructor trims each component and rejects missing provenance. The
/// fields remain private so callers cannot construct an apparently validated
/// value without going through [`EloSourceMetadata::try_new`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EloSourceMetadata {
    source_type: String,
    source_id: String,
    source_version: String,
}

#[derive(Debug, Deserialize)]
struct EloSourceMetadataWire {
    source_type: String,
    source_id: String,
    source_version: String,
}

/// Validation failures for Elo result provenance.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EloSourceMetadataError {
    #[error("Elo source type cannot be empty")]
    Type,
    #[error("Elo source ID cannot be empty")]
    Id,
    #[error("Elo source version cannot be empty")]
    Version,
}

impl<'de> Deserialize<'de> for EloSourceMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EloSourceMetadataWire::deserialize(deserializer)?;
        Self::try_new(wire.source_type, wire.source_id, wire.source_version)
            .map_err(serde::de::Error::custom)
    }
}

impl EloSourceMetadata {
    /// Creates validated source metadata, trimming surrounding whitespace.
    pub fn try_new(
        source_type: impl Into<String>,
        source_id: impl Into<String>,
        source_version: impl Into<String>,
    ) -> Result<Self, EloSourceMetadataError> {
        let source_type = source_type.into().trim().to_owned();
        if source_type.is_empty() {
            return Err(EloSourceMetadataError::Type);
        }

        let source_id = source_id.into().trim().to_owned();
        if source_id.is_empty() {
            return Err(EloSourceMetadataError::Id);
        }

        let source_version = source_version.into().trim().to_owned();
        if source_version.is_empty() {
            return Err(EloSourceMetadataError::Version);
        }

        Ok(Self {
            source_type,
            source_id,
            source_version,
        })
    }

    /// Returns the source category, such as `quiz_attempt` or
    /// `advanced_actual_value`.
    #[must_use]
    pub fn source_type(&self) -> &str {
        &self.source_type
    }

    /// Returns the stable source identity used for audit and idempotency.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Returns the source schema/content version.
    #[must_use]
    pub fn source_version(&self) -> &str {
        &self.source_version
    }

    fn internal_calculation() -> Self {
        Self::try_new("domain_calculation", "unattributed", "1")
            .expect("static internal Elo source metadata is valid")
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        min
    } else {
        value.max(min).min(max)
    }
}

fn normalize_k(k: f64) -> f64 {
    if k.is_finite() && k >= 0.0 {
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

/// The complete result of one deterministic player/question Elo update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EloResult {
    /// Immutable policy version used to calculate this result.
    pub policy_version: u16,
    /// Validated provenance for the facts that produced this result.
    pub source_metadata: EloSourceMetadata,
    /// Raw formula delta before integer rounding: `K * (Sa - Ea)`.
    pub point_delta: f64,
    /// Raw delta rounded to the integer rating-ledger unit.
    pub rounded_delta: i32,
    /// Expected score before applying the outcome.
    pub expected_score: f64,
    /// Actual player change after rounding and player bounds.
    pub player_delta: f64,
    /// Actual question change after rounding and question bounds.
    pub question_delta: f64,
    /// Player rating after applying the bounded update.
    pub player_new_elo: f64,
    /// Question rating after applying the bounded inverse update.
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
/// Ratings are clamped before they participate in the formula:
/// `Ea = 1 / (1 + 10^((Rb - Ra) / 400))`.
#[must_use]
pub fn expected_score(player_elo: f64, question_elo: f64) -> f64 {
    let player_elo = clamp_player_elo(player_elo);
    let question_elo = clamp_question_elo(question_elo);
    1.0 / (1.0 + 10_f64.powf((question_elo - player_elo) / 400.0))
}

/// Calculates one bounded player/question Elo update.
///
/// Intermediate values remain floating point. The raw delta is rounded to the
/// nearest integer using Rust's half-away-from-zero `round` behavior before
/// the player and question deltas are applied independently.
#[must_use]
pub fn compute_elo(player_elo: f64, question_elo: f64, k: f64, sa: f64) -> EloResult {
    compute_elo_with_source(
        player_elo,
        question_elo,
        k,
        sa,
        EloSourceMetadata::internal_calculation(),
    )
}

/// Calculates one bounded Elo update with validated source provenance.
#[must_use]
pub fn compute_elo_with_source(
    player_elo: f64,
    question_elo: f64,
    k: f64,
    sa: f64,
    source_metadata: EloSourceMetadata,
) -> EloResult {
    let player_elo = clamp_player_elo(player_elo);
    let question_elo = clamp_question_elo(question_elo);
    let k = normalize_k(k);
    let sa = normalize_actual_score(sa);
    let expected_score = expected_score(player_elo, question_elo);
    let point_delta = k * (sa - expected_score);
    let rounded_delta = point_delta.round() as i32;
    let player_new_elo = clamp_player_elo(player_elo + f64::from(rounded_delta));
    let question_new_elo = clamp_question_elo(question_elo - f64::from(rounded_delta));

    EloResult {
        policy_version: ELO_POLICY_VERSION,
        source_metadata,
        point_delta,
        rounded_delta,
        expected_score,
        player_delta: player_new_elo - player_elo,
        question_delta: question_new_elo - question_elo,
        player_new_elo,
        question_new_elo,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compute_elo, compute_elo_with_source, expected_score, EloSourceMetadata,
        EloSourceMetadataError, ELO_POLICY_VERSION,
    };

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn expected_score_is_one_half_for_equal_ratings() {
        assert!((expected_score(500.0, 500.0) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn equal_ratings_apply_inverse_basic_deltas() {
        let result = compute_elo(500.0, 500.0, 20.0, 1.0);

        assert_eq!(result.policy_version, 1);
        assert_eq!(result.rounded_delta, 10);
        assert_eq!(result.player_delta, 10.0);
        assert_eq!(result.question_delta, -10.0);
        assert_eq!(result.player_new_elo, 510.0);
        assert_eq!(result.question_new_elo, 490.0);
    }

    #[test]
    fn high_rated_question_can_award_the_full_rounded_delta() {
        let result = compute_elo(1000.0, 2000.0, 30.0, 1.0);

        assert!((result.expected_score - 0.003152309).abs() < 1e-9);
        assert!((result.point_delta - 29.9054307).abs() < 1e-6);
        assert_eq!(result.rounded_delta, 30);
        assert_eq!(result.player_new_elo, 1030.0);
        assert_eq!(result.question_new_elo, 1970.0);
    }

    #[test]
    fn negative_deltas_reverse_the_player_and_question_changes() {
        let result = compute_elo(500.0, 500.0, 35.0, 0.0);

        assert_eq!(result.rounded_delta, -18);
        assert_eq!(result.player_delta, -18.0);
        assert_eq!(result.question_delta, 18.0);
    }

    #[test]
    fn policy_v1_golden_vectors_are_stable() {
        struct GoldenVector {
            name: &'static str,
            player: f64,
            question: f64,
            k: f64,
            sa: f64,
            expected_score: f64,
            raw_delta: f64,
            rounded_delta: i32,
            player_after: f64,
            question_after: f64,
        }

        let vectors = [
            GoldenVector {
                name: "basic_equal_correct",
                player: 500.0,
                question: 500.0,
                k: 20.0,
                sa: 1.0,
                expected_score: 0.5,
                raw_delta: 10.0,
                rounded_delta: 10,
                player_after: 510.0,
                question_after: 490.0,
            },
            GoldenVector {
                name: "basic_equal_incorrect",
                player: 500.0,
                question: 500.0,
                k: 20.0,
                sa: 0.0,
                expected_score: 0.5,
                raw_delta: -10.0,
                rounded_delta: -10,
                player_after: 490.0,
                question_after: 510.0,
            },
            GoldenVector {
                name: "advanced_high_question_perfect_prediction",
                player: 1000.0,
                question: 2000.0,
                k: 30.0,
                sa: 1.0,
                expected_score: 0.003152309183260211,
                raw_delta: 29.90543072450219,
                rounded_delta: 30,
                player_after: 1030.0,
                question_after: 1970.0,
            },
            GoldenVector {
                name: "advanced_high_player_severe_penalty",
                player: 2000.0,
                question: 1000.0,
                k: 35.0,
                sa: 0.0,
                expected_score: 0.9968476908167398,
                raw_delta: -34.88966917858689,
                rounded_delta: -35,
                player_after: 1965.0,
                question_after: 1035.0,
            },
            GoldenVector {
                name: "advanced_neutral_zone",
                player: 1500.0,
                question: 1700.0,
                k: 0.0,
                sa: 0.0,
                expected_score: 0.2402530733520421,
                raw_delta: 0.0,
                rounded_delta: 0,
                player_after: 1500.0,
                question_after: 1700.0,
            },
        ];

        for vector in vectors {
            let result = compute_elo(vector.player, vector.question, vector.k, vector.sa);

            assert_eq!(result.policy_version, ELO_POLICY_VERSION, "{}", vector.name);
            assert_close(result.expected_score, vector.expected_score, 1e-12);
            assert_close(result.point_delta, vector.raw_delta, 1e-10);
            assert_eq!(
                result.rounded_delta, vector.rounded_delta,
                "{}",
                vector.name
            );
            assert_eq!(
                result.player_new_elo, vector.player_after,
                "{}",
                vector.name
            );
            assert_eq!(
                result.question_new_elo, vector.question_after,
                "{}",
                vector.name
            );
        }
    }

    #[test]
    fn golden_vectors_serialize_to_byte_stable_json() {
        let vectors = [
            (
                "basic_equal_correct",
                compute_elo(500.0, 500.0, 20.0, 1.0),
                br#"{"policy_version":1,"source_metadata":{"source_type":"domain_calculation","source_id":"unattributed","source_version":"1"},"point_delta":10.0,"rounded_delta":10,"expected_score":0.5,"player_delta":10.0,"question_delta":-10.0,"player_new_elo":510.0,"question_new_elo":490.0}"#
                    .as_slice(),
            ),
            (
                "advanced_high_question_perfect_prediction",
                compute_elo(1000.0, 2000.0, 30.0, 1.0),
                br#"{"policy_version":1,"source_metadata":{"source_type":"domain_calculation","source_id":"unattributed","source_version":"1"},"point_delta":29.905430724502192,"rounded_delta":30,"expected_score":0.0031523091832602115,"player_delta":30.0,"question_delta":-30.0,"player_new_elo":1030.0,"question_new_elo":1970.0}"#
                    .as_slice(),
            ),
        ];

        for (name, result, expected_bytes) in vectors {
            let actual_bytes = serde_json::to_vec(&result).expect("Elo result serializes");
            assert_eq!(actual_bytes, expected_bytes, "{name}");
        }
    }

    #[test]
    fn bounds_are_applied_after_rounding_and_can_break_zero_sum_at_edges() {
        let player_min = compute_elo(100.0, 100.0, 35.0, 0.0);
        assert_eq!(player_min.rounded_delta, -18);
        assert_eq!(player_min.player_delta, 0.0);
        assert_eq!(player_min.question_delta, 18.0);
        assert_eq!(player_min.player_new_elo, 100.0);
        assert_eq!(player_min.question_new_elo, 118.0);

        let question_min = compute_elo(100.0, 100.0, 35.0, 1.0);
        assert_eq!(question_min.rounded_delta, 18);
        assert_eq!(question_min.player_delta, 18.0);
        assert_eq!(question_min.question_delta, 0.0);
        assert_eq!(question_min.player_new_elo, 118.0);
        assert_eq!(question_min.question_new_elo, 100.0);

        let player_max = compute_elo(3000.0, 2400.0, 30.0, 1.0);
        assert_eq!(player_max.player_new_elo, 3000.0);
        assert_eq!(player_max.question_new_elo, 2399.0);

        let question_max = compute_elo(3000.0, 2400.0, 35.0, 0.0);
        assert_eq!(question_max.player_new_elo, 2966.0);
        assert_eq!(question_max.question_new_elo, 2400.0);
    }

    #[test]
    fn malformed_formula_inputs_are_normalized_deterministically() {
        let malformed = compute_elo(f64::NAN, f64::INFINITY, -1.0, f64::NAN);
        let normalized = compute_elo(100.0, 2400.0, 0.0, 0.0);

        assert_eq!(malformed, normalized);
        assert_eq!(malformed.policy_version, ELO_POLICY_VERSION);
    }

    #[test]
    fn expected_score_is_monotonic_with_player_rating() {
        assert!(expected_score(100.0, 1500.0) < expected_score(500.0, 1500.0));
        assert!(expected_score(500.0, 1500.0) < expected_score(3000.0, 1500.0));
    }

    #[test]
    fn source_metadata_is_trimmed_and_carried_into_the_result() {
        let source = EloSourceMetadata::try_new(" quiz_attempt ", " attempt-123 ", " v1 ")
            .expect("valid source metadata");
        let result = compute_elo_with_source(500.0, 500.0, 20.0, 1.0, source.clone());

        assert_eq!(result.source_metadata, source);
        assert_eq!(result.source_metadata.source_type(), "quiz_attempt");
        assert_eq!(result.source_metadata.source_id(), "attempt-123");
        assert_eq!(result.source_metadata.source_version(), "v1");
    }

    #[test]
    fn source_metadata_rejects_missing_components() {
        assert_eq!(
            EloSourceMetadata::try_new("", "id", "v1"),
            Err(EloSourceMetadataError::Type)
        );
        assert_eq!(
            EloSourceMetadata::try_new("quiz_attempt", " ", "v1"),
            Err(EloSourceMetadataError::Id)
        );
        assert_eq!(
            EloSourceMetadata::try_new("quiz_attempt", "id", ""),
            Err(EloSourceMetadataError::Version)
        );
    }

    #[test]
    fn result_deserialization_rejects_unvalidated_source_metadata() {
        let json = r#"{
            "policy_version": 1,
            "source_metadata": {
                "source_type": "quiz_attempt",
                "source_id": "",
                "source_version": "v1"
            },
            "point_delta": 10.0,
            "rounded_delta": 10,
            "expected_score": 0.5,
            "player_delta": 10.0,
            "question_delta": -10.0,
            "player_new_elo": 510.0,
            "question_new_elo": 490.0
        }"#;

        assert!(serde_json::from_str::<super::EloResult>(json).is_err());
    }
}
