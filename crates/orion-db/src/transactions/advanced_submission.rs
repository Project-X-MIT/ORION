use serde_json::json;
use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{
    AdvancedPredictionRecord, AdvancedPredictionSubmissionInput, AdvancedSubmissionResult,
    QuizAttempt, QuizType, ATTEMPT_COMPLETED, ATTEMPT_PENDING,
};

use super::rating_transaction::{events_for_attempt, lock_user_rating};

/// Durable event consumed by the Advanced settlement worker.
pub const ADVANCED_SUBMITTED_EVENT_TYPE: &str = "orion.quiz.advanced.submitted";
pub const ADVANCED_SUBMISSION_SCHEMA_VERSION: i32 = 1;

/// Records exact numeric predictions and leaves the attempt pending for the
/// provider-backed worker. No score, Elo change, or rating event is created in
/// this transaction.
pub async fn submit_advanced_predictions(
    pool: &PgPool,
    input: AdvancedPredictionSubmissionInput,
) -> Result<AdvancedSubmissionResult> {
    if input.predictions.is_empty() {
        return Err(validation_error(
            "an Advanced submission needs a prediction",
        ));
    }

    let total_questions = i32::try_from(input.predictions.len())
        .map_err(|_| validation_error("too many Advanced predictions"))?;
    let mut transaction = pool.begin().await?;
    let initial_user_rating = lock_user_rating(&mut transaction, input.user_id).await?;

    let _claimed = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO quiz_attempts (
            id, user_id, quiz_type, status, total_questions, correct_answers,
            score, rating_before, rating_after, started_at, created_at, updated_at
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

    let attempt = sqlx::query_as::<_, QuizAttempt>(
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

    if attempt.quiz_type != QuizType::ADVANCED {
        return Err(validation_error(
            "attempt quiz type does not match Advanced submission",
        ));
    }
    if attempt.total_questions != total_questions {
        return Err(validation_error(
            "attempt question count does not match submission",
        ));
    }
    if attempt.status == ATTEMPT_COMPLETED {
        let events = events_for_attempt(&mut transaction, attempt.id).await?;
        transaction.commit().await?;
        return Ok(AdvancedSubmissionResult::Completed(
            crate::models::QuizSettlementResult {
                attempt,
                user_rating: initial_user_rating,
                events,
            },
        ));
    }
    if attempt.status != ATTEMPT_PENDING {
        return Err(validation_error("Advanced attempt is not pending"));
    }

    validate_numeric_questions(&mut transaction, &input.predictions).await?;
    ensure_prediction_rows(&mut transaction, input.attempt_id, &input.predictions).await?;

    let question_ids = input
        .predictions
        .iter()
        .map(|prediction| prediction.question_id)
        .collect::<Vec<_>>();
    let dedupe_key = format!("advanced-submission:{}", input.attempt_id);
    let payload = json!({
        "schema_version": ADVANCED_SUBMISSION_SCHEMA_VERSION,
        "dedupe_key": dedupe_key,
        "attempt_id": input.attempt_id,
        "user_id": input.user_id,
        "question_ids": question_ids,
    });
    sqlx::query(
        r#"
        INSERT INTO outbox_events (event_type, schema_version, payload, request_id)
        SELECT $1, $2, $3, $4
        WHERE NOT EXISTS (
            SELECT 1 FROM outbox_events
            WHERE event_type = $1 AND payload ->> 'dedupe_key' = $5
        )
        "#,
    )
    .bind(ADVANCED_SUBMITTED_EVENT_TYPE)
    .bind(ADVANCED_SUBMISSION_SCHEMA_VERSION)
    .bind(sqlx::types::Json(payload))
    .bind(input.request_id)
    .bind(&dedupe_key)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(AdvancedSubmissionResult::Pending {
        attempt,
        user_rating: initial_user_rating,
    })
}

async fn validate_numeric_questions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    predictions: &[crate::models::AdvancedPredictionSubmission],
) -> Result<()> {
    for prediction in predictions {
        let is_numeric = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM quiz_questions AS question
                WHERE question.id = $1
                  AND question.quiz_type = 'advanced'
                  AND question.active = TRUE
                  AND NOT EXISTS (
                      SELECT 1 FROM quiz_options AS option
                      WHERE option.question_id = question.id
                  )
            )
            "#,
        )
        .bind(prediction.question_id)
        .fetch_one(&mut **transaction)
        .await?;
        if !is_numeric {
            return Err(validation_error(
                "numeric predictions must reference active Advanced numeric questions",
            ));
        }
    }
    Ok(())
}

async fn ensure_prediction_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    attempt_id: Uuid,
    predictions: &[crate::models::AdvancedPredictionSubmission],
) -> Result<()> {
    let existing = sqlx::query_as::<_, AdvancedPredictionRecord>(
        "SELECT attempt_id, question_id, value, submitted_at
         FROM advanced_predictions
         WHERE attempt_id = $1
         FOR UPDATE",
    )
    .bind(attempt_id)
    .fetch_all(&mut **transaction)
    .await?;

    let requested = predictions.len();
    if !existing.is_empty() {
        if existing.len() != requested
            || existing.iter().any(|row| {
                !predictions.iter().any(|prediction| {
                    prediction.question_id == row.question_id && prediction.value == row.value
                })
            })
        {
            return Err(validation_error(
                "attempt id is already bound to a different prediction payload",
            ));
        }
        return Ok(());
    }

    for prediction in predictions {
        sqlx::query(
            "INSERT INTO advanced_predictions
             (attempt_id, question_id, value, submitted_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(attempt_id)
        .bind(prediction.question_id)
        .bind(prediction.value)
        .bind(prediction.submitted_at)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

fn validation_error(message: &str) -> sqlx::Error {
    sqlx::Error::Protocol(message.to_owned())
}
