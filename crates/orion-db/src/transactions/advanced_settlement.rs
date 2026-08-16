use std::collections::HashSet;

use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{
    AdvancedSettlementInput, QuizAttempt, QuizSettlementInput, QuizSettlementResult, QuizType,
    ATTEMPT_COMPLETED, ATTEMPT_PENDING,
};

use super::basic_settlement::settle;
use super::rating_transaction::{
    apply_advanced_rating_change, events_for_attempt, lock_user_rating, ADVANCED_K_FACTOR,
};

/// Settles an Advanced Quiz attempt atomically.
///
/// Advanced attempts use the same zero-sum user/question Elo model but a
/// larger K factor, so difficult questions adapt faster to observed results.
pub async fn settle_advanced_quiz(
    pool: &PgPool,
    input: QuizSettlementInput,
) -> Result<QuizSettlementResult> {
    settle(pool, &input, QuizType::Advanced, ADVANCED_K_FACTOR).await
}

pub async fn settle_advanced_attempt(
    pool: &PgPool,
    input: QuizSettlementInput,
) -> Result<QuizSettlementResult> {
    settle_advanced_quiz(pool, input).await
}

/// Settles a pending Advanced attempt from validated numeric actual values.
///
/// The worker obtains and validates provider values, then calls this function
/// exactly once for the atomic business operation. This transaction owns the
/// user/question locks, shared domain scorer invocation, immutable rating
/// events, ledger rows, and pending-to-completed transition.
pub async fn settle_advanced_actual_quiz(
    pool: &PgPool,
    input: AdvancedSettlementInput,
) -> Result<QuizSettlementResult> {
    if input.resolutions.is_empty() {
        return Err(sqlx::Error::Protocol(
            "an Advanced attempt must contain a resolution".to_owned(),
        ));
    }
    let total_questions = i32::try_from(input.resolutions.len())
        .map_err(|_| sqlx::Error::Protocol("too many Advanced resolutions".to_owned()))?;

    let mut resolutions = input.resolutions;
    resolutions.sort_unstable_by_key(|resolution| resolution.prediction.question_id);

    let mut transaction = pool.begin().await?;
    let initial_user_rating = lock_user_rating(&mut transaction, input.user_id).await?;
    let claimed = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO quiz_attempts (
            id,
            user_id,
            quiz_type,
            status,
            total_questions,
            correct_answers,
            score,
            rating_before,
            rating_after,
            started_at,
            created_at,
            updated_at
        )
        VALUES ($1, $2, 'advanced', $3, $4, 0, 0, $5, $5, $6, $6, $6)
        ON CONFLICT (id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(input.attempt_id)
    .bind(input.user_id)
    .bind(ATTEMPT_PENDING)
    .bind(total_questions)
    .bind(initial_user_rating.rating)
    .bind(input.started_at)
    .fetch_optional(&mut *transaction)
    .await?;

    if claimed.is_none() {
        let existing = sqlx::query_as::<_, QuizAttempt>(
            r#"
            SELECT id, user_id, quiz_type, status, total_questions, correct_answers,
                   score, rating_before, rating_after, started_at, completed_at,
                   created_at, updated_at
            FROM quiz_attempts
            WHERE id = $1 AND user_id = $2
            FOR UPDATE
            "#,
        )
        .bind(input.attempt_id)
        .bind(input.user_id)
        .fetch_one(&mut *transaction)
        .await?;
        if existing.quiz_type != QuizType::ADVANCED {
            return Err(sqlx::Error::Protocol(
                "attempt quiz type does not match Advanced settlement".to_owned(),
            ));
        }
        if existing.total_questions != total_questions {
            return Err(sqlx::Error::Protocol(
                "attempt question count does not match Advanced settlement".to_owned(),
            ));
        }
        if existing.status == ATTEMPT_COMPLETED {
            let result = existing_result(&mut transaction, existing).await?;
            transaction.commit().await?;
            return Ok(result);
        }
        if existing.status != ATTEMPT_PENDING {
            return Err(sqlx::Error::Protocol(
                "Advanced attempt is not pending".to_owned(),
            ));
        }
    }

    let mut seen_questions = HashSet::with_capacity(resolutions.len());
    let mut scored_count = 0_i32;
    let mut events = Vec::with_capacity(resolutions.len());
    for resolution in &resolutions {
        if resolution.prediction.question_id != resolution.actual.question_id
            || !seen_questions.insert(resolution.prediction.question_id)
            || !resolution.actual.is_final
            || resolution.actual.available_at < resolution.actual.observed_at
            || resolution.actual.source_id.trim().is_empty()
            || resolution.actual.source_version.trim().is_empty()
        {
            return Err(sqlx::Error::Protocol(
                "Advanced resolution failed atomic validation".to_owned(),
            ));
        }
        let event = apply_advanced_rating_change(
            &mut transaction,
            input.attempt_id,
            input.user_id,
            resolution,
            input.completed_at,
        )
        .await?;
        scored_count += i32::from(event.outcome);
        events.push(event);
    }

    let final_user_rating = lock_user_rating(&mut transaction, input.user_id).await?;
    let score = scored_count * 100 / total_questions;
    sqlx::query(
        r#"
        UPDATE quiz_attempts
        SET status = $2,
            correct_answers = $3,
            score = $4,
            rating_after = $5,
            completed_at = $6,
            updated_at = $6
        WHERE id = $1
          AND user_id = $7
          AND status = $8
        "#,
    )
    .bind(input.attempt_id)
    .bind(ATTEMPT_COMPLETED)
    .bind(scored_count)
    .bind(score)
    .bind(final_user_rating.rating)
    .bind(input.completed_at)
    .bind(input.user_id)
    .bind(ATTEMPT_PENDING)
    .execute(&mut *transaction)
    .await?;

    let attempt = sqlx::query_as::<_, QuizAttempt>(
        r#"
        SELECT id, user_id, quiz_type, status, total_questions, correct_answers,
               score, rating_before, rating_after, started_at, completed_at,
               created_at, updated_at
        FROM quiz_attempts
        WHERE id = $1
        "#,
    )
    .bind(input.attempt_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(QuizSettlementResult {
        attempt,
        user_rating: final_user_rating,
        events,
    })
}

async fn existing_result(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    attempt: QuizAttempt,
) -> Result<QuizSettlementResult> {
    let user_rating = lock_user_rating(transaction, attempt.user_id).await?;
    let events = events_for_attempt(transaction, attempt.id).await?;
    Ok(QuizSettlementResult {
        attempt,
        user_rating,
        events,
    })
}
