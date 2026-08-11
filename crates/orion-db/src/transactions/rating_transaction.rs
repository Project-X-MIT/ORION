use chrono::{DateTime, Utc};
use orion_domain::value_objects::elo::{compute_elo_with_source, EloSourceMetadata, BASIC_ELO_K};
use sqlx::{Postgres, Result, Transaction};
use uuid::Uuid;

use crate::models::{QuestionRating, RatingEvent, UserRating, DEFAULT_RATING};

/// Default K factors used by the two quiz modes.
pub const BASIC_K_FACTOR: i32 = BASIC_ELO_K as i32;
/// Legacy integer fallback used by the current DB-only Advanced settlement
/// path. Zone-based Advanced calculation is owned by the domain scorer.
pub const ADVANCED_K_FACTOR: i32 = 40;

pub const PLAYER_ELO_MIN: i32 = 100;
pub const PLAYER_ELO_MAX: i32 = 3000;
pub const QUESTION_ELO_MIN: i32 = 100;
pub const QUESTION_ELO_MAX: i32 = 2400;

async fn ensure_user_rating(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<UserRating> {
    sqlx::query(
        r#"
        INSERT INTO user_ratings (user_id, rating)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(DEFAULT_RATING)
    .execute(&mut **transaction)
    .await?;

    sqlx::query_as::<_, UserRating>(
        r#"
        SELECT user_id, rating, games_played, wins, losses, draws, created_at, updated_at
        FROM user_ratings
        WHERE user_id = $1
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await
}

async fn ensure_question_rating(
    transaction: &mut Transaction<'_, Postgres>,
    question_id: Uuid,
) -> Result<QuestionRating> {
    sqlx::query(
        r#"
        INSERT INTO question_ratings (question_id, rating)
        VALUES ($1, $2)
        ON CONFLICT (question_id) DO NOTHING
        "#,
    )
    .bind(question_id)
    .bind(DEFAULT_RATING)
    .execute(&mut **transaction)
    .await?;

    sqlx::query_as::<_, QuestionRating>(
        r#"
        SELECT question_id, rating, attempts, correct_answers, created_at, updated_at
        FROM question_ratings
        WHERE question_id = $1
        FOR UPDATE
        "#,
    )
    .bind(question_id)
    .fetch_one(&mut **transaction)
    .await
}

#[allow(clippy::too_many_arguments)]
async fn append_rating_ledger(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    source_type: &str,
    source_id: Uuid,
    dedupe_key: &str,
    rating_before: i32,
    rating_after: i32,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO rating_ledger (
            user_id, source_type, source_id, dedupe_key,
            rating_before, rating_after, rating_delta, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(user_id)
    .bind(source_type)
    .bind(source_id)
    .bind(dedupe_key)
    .bind(rating_before)
    .bind(rating_after)
    .bind(rating_after - rating_before)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Applies one user/question Elo result and records the complete audit event.
///
/// Both rating rows are inserted if necessary and locked before the values are
/// read. Callers can therefore process all answers in one surrounding database
/// transaction without lost updates or partially-written history.
#[allow(clippy::too_many_arguments)]
pub async fn apply_rating_change(
    transaction: &mut Transaction<'_, Postgres>,
    attempt_id: Option<Uuid>,
    user_id: Uuid,
    question_id: Uuid,
    quiz_type: &str,
    outcome: bool,
    k_factor: i32,
    now: DateTime<Utc>,
) -> Result<RatingEvent> {
    let user = ensure_user_rating(transaction, user_id).await?;
    let question = ensure_question_rating(transaction, question_id).await?;
    let source_type = "quiz_attempt";
    let source_id = attempt_id.unwrap_or_else(Uuid::new_v4);
    let source_metadata = EloSourceMetadata::try_new(source_type, source_id.to_string(), "1")
        .expect("static quiz Elo source metadata is valid");
    let elo_result = compute_elo_with_source(
        f64::from(user.rating),
        f64::from(question.rating),
        f64::from(k_factor),
        if outcome { 1.0 } else { 0.0 },
        source_metadata,
    );
    // Persist the bounded outputs from the shared calculation. The database
    // transaction owns locking, counters, ledger writes, and idempotency; it
    // does not recompute the Elo result.
    let user_after = elo_result.player_new_elo as i32;
    let question_after = elo_result.question_new_elo as i32;
    let effective_user_delta = user_after - user.rating;
    let outcome_value = if outcome { 1_i16 } else { 0_i16 };
    let error_pct = if question.attempts == 0 {
        0.0
    } else {
        ((question.attempts - question.correct_answers) as f64 / question.attempts as f64) * 100.0
    };
    let zone = quiz_type;
    let sa = if outcome { 1.0 } else { 0.0 };

    sqlx::query(
        r#"
        UPDATE user_ratings
        SET
            rating = $2,
            games_played = games_played + 1,
            wins = wins + CASE WHEN $3 = 1 THEN 1 ELSE 0 END,
            losses = losses + CASE WHEN $3 = 0 THEN 1 ELSE 0 END,
            updated_at = $4
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(user_after)
    .bind(outcome_value)
    .bind(now)
    .execute(&mut **transaction)
    .await?;

    sqlx::query(
        r#"
        UPDATE question_ratings
        SET
            rating = $2,
            attempts = attempts + 1,
            correct_answers = correct_answers + $3,
            updated_at = $4
        WHERE question_id = $1
        "#,
    )
    .bind(question_id)
    .bind(question_after)
    .bind(i32::from(outcome_value))
    .bind(now)
    .execute(&mut **transaction)
    .await?;

    append_rating_ledger(
        transaction,
        user_id,
        source_type,
        source_id,
        &question_id.to_string(),
        user.rating,
        user_after,
        now,
    )
    .await?;

    sqlx::query_as::<_, RatingEvent>(
        r#"
        INSERT INTO rating_events (
            id,
            attempt_id,
            user_id,
            question_id,
            source_type,
            source_id,
            quiz_type,
            outcome,
            correct,
            zone,
            error_pct,
            k,
            sa,
            point_delta,
            user_rating_before,
            user_rating_after,
            player_elo_before,
            player_elo_after,
            question_rating_before,
            question_rating_after,
            question_elo_before,
            question_elo_after,
            rating_delta,
            created_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24
        )
        RETURNING
            id,
            attempt_id,
            user_id,
            question_id,
            source_type,
            source_id,
            quiz_type,
            outcome,
            correct,
            zone,
            error_pct,
            k,
            sa,
            point_delta,
            user_rating_before,
            user_rating_after,
            player_elo_before,
            player_elo_after,
            question_rating_before,
            question_rating_after,
            question_elo_before,
            question_elo_after,
            rating_delta,
            created_at
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(attempt_id)
    .bind(user_id)
    .bind(question_id)
    .bind(source_type)
    .bind(source_id)
    .bind(quiz_type)
    .bind(outcome_value)
    .bind(outcome)
    .bind(zone)
    .bind(error_pct)
    .bind(k_factor)
    .bind(sa)
    .bind(effective_user_delta)
    .bind(user.rating)
    .bind(user_after)
    .bind(user.rating)
    .bind(user_after)
    .bind(question.rating)
    .bind(question_after)
    .bind(question.rating)
    .bind(question_after)
    .bind(effective_user_delta)
    .bind(now)
    .fetch_one(&mut **transaction)
    .await
}

/// Reads and locks the current user rating for a settlement.
pub(crate) async fn lock_user_rating(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<UserRating> {
    ensure_user_rating(transaction, user_id).await
}

/// Reads one completed attempt's audit rows without opening a second pool
/// connection. This is used by idempotent settlement retries.
pub(crate) async fn events_for_attempt(
    transaction: &mut Transaction<'_, Postgres>,
    attempt_id: Uuid,
) -> Result<Vec<RatingEvent>> {
    sqlx::query_as::<_, RatingEvent>(
        r#"
        SELECT id, attempt_id, user_id, question_id,
               source_type, source_id, quiz_type, outcome,
               correct, zone, error_pct, k, sa, point_delta,
               user_rating_before, user_rating_after,
               player_elo_before, player_elo_after,
               question_rating_before, question_rating_after,
               question_elo_before, question_elo_after,
               rating_delta, created_at
        FROM rating_events
        WHERE attempt_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(attempt_id)
    .fetch_all(&mut **transaction)
    .await
}

#[cfg(test)]
mod tests {
    use orion_domain::value_objects::elo::{compute_elo, expected_score};

    use super::BASIC_K_FACTOR;

    #[test]
    fn basic_k_factor_matches_the_approved_policy() {
        assert_eq!(BASIC_K_FACTOR, 20);
    }

    #[test]
    fn equal_ratings_have_equal_expected_score() {
        assert!((expected_score(1200.0, 1200.0) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn equal_ratings_move_by_half_the_k_factor() {
        assert_eq!(compute_elo(1200.0, 1200.0, 32.0, 1.0).rounded_delta, 16);
        assert_eq!(compute_elo(1200.0, 1200.0, 32.0, 0.0).rounded_delta, -16);
    }

    #[test]
    fn synthetic_answer_sequence_preserves_zero_sum_elo() {
        let answers = [true, false, true];
        let mut player_rating = 1200;
        let mut question_rating = 1200;

        for correct in answers {
            let player_before = player_rating;
            let question_before = question_rating;
            let delta = compute_elo(
                f64::from(player_before),
                f64::from(question_before),
                32.0,
                if correct { 1.0 } else { 0.0 },
            )
            .rounded_delta;

            player_rating += delta;
            question_rating -= delta;

            assert_eq!(player_rating - player_before, delta);
            assert_eq!(question_rating - question_before, -delta);
            assert_eq!(player_rating + question_rating, 2400);
        }

        assert_eq!(player_rating, 1215);
        assert_eq!(question_rating, 1185);
    }
}
