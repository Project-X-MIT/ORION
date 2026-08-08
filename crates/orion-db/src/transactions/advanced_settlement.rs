use sqlx::{PgPool, Result};

use crate::models::{AdvancedSettlementInput, QuizSettlementResult, QuizType};

use super::basic_settlement::settle;
use super::rating_transaction::ADVANCED_K_FACTOR;

/// Settles an Advanced Quiz attempt atomically.
///
/// Advanced attempts use the same zero-sum user/question Elo model but a
/// larger K factor, so difficult questions adapt faster to observed results.
pub async fn settle_advanced_quiz(
    pool: &PgPool,
    input: AdvancedSettlementInput,
) -> Result<QuizSettlementResult> {
    settle(pool, &input, QuizType::Advanced, ADVANCED_K_FACTOR).await
}

pub async fn settle_advanced_attempt(
    pool: &PgPool,
    input: AdvancedSettlementInput,
) -> Result<QuizSettlementResult> {
    settle_advanced_quiz(pool, input).await
}
