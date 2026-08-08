use std::collections::HashSet;

use sqlx::{PgPool, Postgres, Result, Transaction};
use uuid::Uuid;

use crate::models::{
    BasicSettlementInput, QuizAnswer, QuizAttempt, QuizSettlementResult, QuizType, RatingEvent,
    ATTEMPT_COMPLETED, ATTEMPT_PENDING,
};

use super::rating_transaction::{
    apply_rating_change, events_for_attempt, lock_user_rating, BASIC_K_FACTOR,
};

fn validation_error(message: &str) -> sqlx::Error {
    sqlx::Error::Protocol(message.to_owned())
}

async fn answer_is_correct(
    transaction: &mut Transaction<'_, Postgres>,
    answer: QuizAnswer,
    quiz_type: QuizType,
) -> Result<bool> {
    match answer.option_id {
        Some(option_id) => sqlx::query_scalar::<_, bool>(
            r#"
                SELECT quiz_option.is_correct
                FROM quiz_questions AS question
                INNER JOIN quiz_options AS quiz_option
                    ON quiz_option.question_id = question.id
                   AND quiz_option.id = $2
                WHERE question.id = $1
                  AND question.quiz_type = $3
                  AND question.active = TRUE
                "#,
        )
        .bind(answer.question_id)
        .bind(option_id)
        .bind(quiz_type.as_str())
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(sqlx::Error::RowNotFound),
        None => {
            let exists = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT TRUE
                FROM quiz_questions
                WHERE id = $1
                  AND quiz_type = $2
                  AND active = TRUE
                "#,
            )
            .bind(answer.question_id)
            .bind(quiz_type.as_str())
            .fetch_optional(&mut **transaction)
            .await?;
            exists.ok_or(sqlx::Error::RowNotFound).map(|_| false)
        }
    }
}

async fn existing_result(
    transaction: &mut Transaction<'_, Postgres>,
    attempt: QuizAttempt,
    quiz_type: QuizType,
) -> Result<QuizSettlementResult> {
    if attempt.status != ATTEMPT_COMPLETED || attempt.quiz_type != quiz_type.as_str() {
        return Err(validation_error("quiz attempt is already being settled"));
    }
    let user_rating = lock_user_rating(transaction, attempt.user_id).await?;
    let events = events_for_attempt(transaction, attempt.id).await?;
    Ok(QuizSettlementResult {
        attempt,
        user_rating,
        events,
    })
}

/// Settles one quiz attempt and all of its Elo updates in one transaction.
///
/// The attempt UUID is claimed before any rating is changed. Repeating a
/// request with the same UUID returns the already-completed result, which
/// makes API retries safe and prevents double-rating.
pub(crate) async fn settle(
    pool: &PgPool,
    input: &BasicSettlementInput,
    quiz_type: QuizType,
    k_factor: i32,
) -> Result<QuizSettlementResult> {
    if input.answers.is_empty() {
        return Err(validation_error("a quiz attempt must contain an answer"));
    }
    let total_questions =
        i32::try_from(input.answers.len()).map_err(|_| validation_error("too many answers"))?;

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
        VALUES ($1, $2, $3, $4, $5, 0, 0, $6, $6, $7, $7, $7)
        ON CONFLICT (id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(input.attempt_id)
    .bind(input.user_id)
    .bind(quiz_type.as_str())
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
        if existing.quiz_type != quiz_type.as_str() {
            return Err(validation_error(
                "attempt quiz type does not match settlement",
            ));
        }
        if existing.total_questions != total_questions {
            return Err(validation_error(
                "attempt question count does not match settlement",
            ));
        }
        if existing.status == ATTEMPT_COMPLETED {
            let result = existing_result(&mut transaction, existing, quiz_type).await?;
            transaction.commit().await?;
            return Ok(result);
        }
    }

    // Every settlement locks question ratings in UUID order. This keeps two
    // concurrent attempts containing the same questions from deadlocking when
    // their client-side answer order differs.
    let mut answers = input.answers.clone();
    answers.sort_unstable_by_key(|answer| answer.question_id);

    let mut seen_questions = HashSet::with_capacity(answers.len());
    let mut correct_answers = 0_i32;
    let mut events = Vec::<RatingEvent>::with_capacity(answers.len());

    for answer in answers.iter().copied() {
        if !seen_questions.insert(answer.question_id) {
            return Err(validation_error("a question may only be answered once"));
        }
        let correct = answer_is_correct(&mut transaction, answer, quiz_type).await?;
        if correct {
            correct_answers += 1;
        }
        events.push(
            apply_rating_change(
                &mut transaction,
                Some(input.attempt_id),
                input.user_id,
                answer.question_id,
                quiz_type.as_str(),
                correct,
                k_factor,
                input.completed_at,
            )
            .await?,
        );
    }

    let score = correct_answers * 100 / total_questions;
    let final_user_rating = lock_user_rating(&mut transaction, input.user_id).await?;

    sqlx::query(
        r#"
        UPDATE quiz_attempts
        SET
            status = $2,
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
    .bind(correct_answers)
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

/// Settles a Basic Quiz attempt using the basic Elo K factor.
pub async fn settle_basic_quiz(
    pool: &PgPool,
    input: BasicSettlementInput,
) -> Result<QuizSettlementResult> {
    settle(pool, &input, QuizType::Basic, BASIC_K_FACTOR).await
}

/// Alias emphasizing that the UUID identifies an attempt, not a session.
pub async fn settle_basic_attempt(
    pool: &PgPool,
    input: BasicSettlementInput,
) -> Result<QuizSettlementResult> {
    settle_basic_quiz(pool, input).await
}
