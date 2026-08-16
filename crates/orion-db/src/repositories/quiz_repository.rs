use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::{
    models::{
        AdvancedPredictionSubmissionInput, AdvancedSubmissionResult, QuizAttempt, QuizQuestion,
        QuizQuestionWithOptions, QuizSettlementInput, QuizSettlementResult, QuizType,
    },
    queries::{quiz_attempts, quiz_questions, ratings},
    transactions,
};

/// Persistence gateway for quiz question reads and atomic quiz settlement.
///
/// Route handlers use this façade instead of reaching into SQL queries or
/// transactions directly. PostgreSQL remains authoritative for both quiz and
/// rating state.
#[derive(Debug, Clone)]
pub struct QuizRepository {
    pool: PgPool,
}

impl QuizRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns active Basic Quiz questions with their options and ratings.
    pub async fn basic_questions_with_options(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<QuizQuestionWithOptions>> {
        quiz_questions::basic_questions_with_options(&self.pool, limit, offset).await
    }

    /// Returns active Advanced Quiz questions with their options and ratings.
    pub async fn advanced_questions_with_options(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<QuizQuestionWithOptions>> {
        quiz_questions::advanced_questions_with_options(&self.pool, limit, offset).await
    }

    /// Returns only the active question rows for a cache-assisted page. The
    /// per-question options/rating read can then be served from Redis without
    /// making Redis authoritative for pagination.
    pub async fn questions(
        &self,
        quiz_type: QuizType,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<QuizQuestion>> {
        quiz_questions::list_by_type(&self.pool, quiz_type, limit, offset).await
    }

    /// Loads one authoritative question projection for a cache miss or
    /// rebuild. PostgreSQL remains the source of truth for all fields.
    pub async fn question_with_options(
        &self,
        question_id: Uuid,
    ) -> Result<Option<QuizQuestionWithOptions>> {
        quiz_questions::find_with_options(&self.pool, question_id).await
    }

    /// Settles one Basic Quiz attempt and all related rating changes in one
    /// database transaction.
    pub async fn settle_basic(&self, input: QuizSettlementInput) -> Result<QuizSettlementResult> {
        transactions::settle_basic_quiz(&self.pool, input).await
    }

    /// Settles one Advanced Quiz prediction attempt and its rating changes in
    /// one database transaction.
    pub async fn settle_advanced(
        &self,
        input: QuizSettlementInput,
    ) -> Result<QuizSettlementResult> {
        transactions::settle_advanced_quiz(&self.pool, input).await
    }

    /// Records exact numeric Advanced predictions and leaves settlement to the
    /// provider-backed worker.
    pub async fn submit_advanced_predictions(
        &self,
        input: AdvancedPredictionSubmissionInput,
    ) -> Result<AdvancedSubmissionResult> {
        transactions::submit_advanced_predictions(&self.pool, input).await
    }

    /// Returns a completed attempt owned by the authenticated user.
    pub async fn find_completed_basic_attempt(
        &self,
        attempt_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<QuizAttempt>> {
        let attempt = quiz_attempts::find_completed_by_id(&self.pool, attempt_id, user_id).await?;
        Ok(attempt.filter(|attempt| attempt.quiz_type == "basic"))
    }

    /// Returns a completed Advanced attempt owned by the authenticated user.
    pub async fn find_completed_advanced_attempt(
        &self,
        attempt_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<QuizAttempt>> {
        let attempt = quiz_attempts::find_completed_by_id(&self.pool, attempt_id, user_id).await?;
        Ok(attempt.filter(|attempt| attempt.quiz_type == "advanced"))
    }

    /// Returns the completed result and immutable rating events for an attempt
    /// owned by the authenticated user.
    pub async fn find_completed_result(
        &self,
        attempt_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<QuizSettlementResult>> {
        let Some(attempt) =
            quiz_attempts::find_completed_by_id(&self.pool, attempt_id, user_id).await?
        else {
            return Ok(None);
        };
        let Some(user_rating) = ratings::get_user_rating(&self.pool, user_id).await? else {
            return Err(sqlx::Error::RowNotFound);
        };
        let events = ratings::rating_events_by_attempt_id(&self.pool, attempt_id).await?;

        Ok(Some(QuizSettlementResult {
            attempt,
            user_rating,
            events,
        }))
    }
}
