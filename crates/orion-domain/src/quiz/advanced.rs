use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::basic::SpecificationTopic;

/// Maximum number of fractional digits accepted for an Advanced value.
pub const MAX_ADVANCED_DECIMAL_SCALE: u32 = 18;

/// A stable unit/currency contract for an Advanced prediction.
///
/// The contract is copied into the question at creation time. Predictions
/// and actual values must use the same contract before they can be scored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedValueSpec {
    pub unit_code: String,
    pub currency_code: Option<String>,
    pub scale: u32,
}

impl AdvancedValueSpec {
    /// Creates a validated value contract.
    pub fn new(
        unit_code: impl Into<String>,
        currency_code: Option<String>,
        scale: u32,
    ) -> Result<Self, AdvancedValueSpecError> {
        let spec = Self {
            unit_code: unit_code.into(),
            currency_code,
            scale,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Validates the shape of the unit contract without changing it.
    pub fn validate(&self) -> Result<(), AdvancedValueSpecError> {
        if self.unit_code.trim().is_empty() {
            return Err(AdvancedValueSpecError::EmptyUnitCode);
        }
        if !self
            .unit_code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/' | b'.'))
        {
            return Err(AdvancedValueSpecError::InvalidUnitCode {
                unit_code: self.unit_code.clone(),
            });
        }
        if self.scale > MAX_ADVANCED_DECIMAL_SCALE {
            return Err(AdvancedValueSpecError::ScaleTooLarge { scale: self.scale });
        }
        if let Some(currency_code) = &self.currency_code {
            if currency_code.len() != 3
                || !currency_code.bytes().all(|byte| byte.is_ascii_uppercase())
            {
                return Err(AdvancedValueSpecError::InvalidCurrencyCode {
                    currency_code: currency_code.clone(),
                });
            }
        }
        Ok(())
    }

    /// Returns whether a decimal can be represented without rounding.
    #[must_use]
    pub fn accepts(&self, value: Decimal) -> bool {
        value.scale() <= self.scale
    }
}

/// Invalid unit/currency/precision configuration for an Advanced question.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdvancedValueSpecError {
    #[error("Advanced value unit code cannot be empty")]
    EmptyUnitCode,

    #[error("Advanced value unit code {unit_code:?} contains unsupported characters")]
    InvalidUnitCode { unit_code: String },

    #[error("Advanced value scale {scale} exceeds the maximum of {MAX_ADVANCED_DECIMAL_SCALE}")]
    ScaleTooLarge { scale: u32 },

    #[error("Advanced currency code {currency_code:?} must be three uppercase letters")]
    InvalidCurrencyCode { currency_code: String },
}

/// An Advanced question whose answer is an exact decimal value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedQuestion {
    pub id: Uuid,
    pub topic: SpecificationTopic,
    pub prompt: String,
    pub value_spec: AdvancedValueSpec,
    pub market_calendar_id: String,
    pub market_calendar_version: String,
    pub market_timezone: String,
    pub horizon_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A submitted Advanced prediction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedPrediction {
    pub question_id: Uuid,
    #[serde(with = "rust_decimal::serde::str")]
    pub value: Decimal,
    pub submitted_at: DateTime<Utc>,
}

impl AdvancedPrediction {
    #[must_use]
    pub const fn new(question_id: Uuid, value: Decimal, submitted_at: DateTime<Utc>) -> Self {
        Self {
            question_id,
            value,
            submitted_at,
        }
    }

    /// Alias that makes the prediction intent explicit at call sites.
    #[must_use]
    pub const fn predicted_value(&self) -> Decimal {
        self.value
    }
}

/// A source-backed actual value used to resolve an Advanced prediction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedActualValue {
    pub question_id: Uuid,
    #[serde(with = "rust_decimal::serde::str")]
    pub value: Decimal,
    pub observed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub source_id: String,
    pub source_version: String,
    pub is_final: bool,
}

impl AdvancedActualValue {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        question_id: Uuid,
        value: Decimal,
        observed_at: DateTime<Utc>,
        available_at: DateTime<Utc>,
        source_id: String,
        source_version: String,
        is_final: bool,
    ) -> Self {
        Self {
            question_id,
            value,
            observed_at,
            available_at,
            source_id,
            source_version,
            is_final,
        }
    }
}

/// Lifecycle states used while an Advanced actual value is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvancedLifecycleState {
    Submitted,
    Pending,
    ActualAvailable,
    Scored,
    Settled,
    Delayed,
    Expired,
    Corrected,
}

/// Mode-specific aliases for callers that use generic quiz terminology.
pub type AdvancedAnswer = AdvancedPrediction;
pub type AdvancedValue = AdvancedActualValue;

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use super::{AdvancedPrediction, AdvancedValueSpec, MAX_ADVANCED_DECIMAL_SCALE};

    #[test]
    fn value_spec_accepts_exact_unit_contract_boundaries() {
        let spec =
            AdvancedValueSpec::new("price", Some("USD".to_owned()), MAX_ADVANCED_DECIMAL_SCALE)
                .expect("valid value spec");

        assert!(spec.accepts(Decimal::new(1, MAX_ADVANCED_DECIMAL_SCALE)));
        assert!(!spec.accepts(Decimal::new(1, MAX_ADVANCED_DECIMAL_SCALE + 1)));
    }

    #[test]
    fn prediction_serializes_decimal_as_a_string() {
        let prediction = AdvancedPrediction::new(
            Uuid::from_u128(1),
            Decimal::new(12345, 2),
            Utc.with_ymd_and_hms(2026, 8, 10, 9, 0, 0).unwrap(),
        );

        let json = serde_json::to_value(prediction).expect("prediction serializes");
        assert_eq!(json["value"], "123.45");
    }

    #[test]
    fn prediction_preserves_timestamp_precision() {
        let submitted_at = Utc::now() + Duration::microseconds(1);
        let prediction = AdvancedPrediction::new(Uuid::from_u128(1), Decimal::ONE, submitted_at);

        assert_eq!(prediction.submitted_at, submitted_at);
    }
}
