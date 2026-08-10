use chrono::{DateTime, Utc};
use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::{models::ResearchPaper, transactions::rating_transaction};

/// Completes one review and moves the paper to `approved` or `rejected` in
/// the same transaction.  The paper row is locked before the review is
/// upserted, so concurrent evaluator retries cannot make a stale decision.
pub async fn complete_research_review(
    pool: &PgPool,
    paper_id: Uuid,
    reviewer_id: Uuid,
    score: Option<f64>,
    recommendation: &str,
    comments: Option<&str>,
    evaluation_result: Option<&sqlx::types::JsonValue>,
) -> Result<Option<ResearchPaper>> {
    let mut transaction = pool.begin().await?;

    let paper = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT id, author_id, status\n         FROM research_papers\n         WHERE id = $1\n         FOR UPDATE",
    )
    .bind(paper_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some((paper_id, author_id, status)) = paper else {
        transaction.commit().await?;
        return Ok(None);
    };

    if status != "under_review" {
        transaction.commit().await?;
        return Ok(None);
    }

    // A paper author cannot review their own paper.  The migration also
    // installs a trigger for callers that bypass this transaction.
    if author_id == reviewer_id {
        transaction.commit().await?;
        return Ok(None);
    }

    let (recommendation, next_status) = match recommendation {
        "approve" | "approved" => ("approve", "approved"),
        "reject" | "rejected" => ("reject", "rejected"),
        _ => {
            transaction.commit().await?;
            return Ok(None);
        }
    };

    sqlx::query(
        "INSERT INTO research_reviews\n             (paper_id, reviewer_id, score, recommendation, comments, evaluation_result)\n         VALUES ($1, $2, $3, $4, $5, $6)\n         ON CONFLICT (paper_id, reviewer_id) DO UPDATE\n         SET score = EXCLUDED.score,\n             recommendation = EXCLUDED.recommendation,\n             comments = EXCLUDED.comments,\n             evaluation_result = EXCLUDED.evaluation_result,\n             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .bind(score)
    .bind(recommendation)
    .bind(comments)
    .bind(evaluation_result)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "UPDATE research_reviews\n         SET reviewed_at = CURRENT_TIMESTAMP\n         WHERE paper_id = $1 AND reviewer_id = $2",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .execute(&mut *transaction)
    .await?;

    // Keep the normalized aggregate on the paper for list/detail reads while
    // retaining every review row for auditability.
    sqlx::query(
        "UPDATE research_papers\n         SET status = $2,\n             decided_by = $4,\n             decided_at = COALESCE(decided_at, CURRENT_TIMESTAMP),\n             evaluation_score = (\n                 SELECT AVG(score)::double precision\n                 FROM research_reviews\n                 WHERE paper_id = $1 AND score IS NOT NULL\n             ),\n             evaluation_result = $3\n         WHERE id = $1 AND status = 'under_review'",
    )
    .bind(paper_id)
    .bind(next_status)
    .bind(evaluation_result)
    .bind(reviewer_id)
    .execute(&mut *transaction)
    .await?;

    let query = format!(
        "SELECT {} FROM research_papers WHERE id = $1",
        paper_columns()
    );
    let updated = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(&mut *transaction)
        .await?;

    transaction.commit().await?;
    Ok(updated)
}

/// Convenience form for evaluators that do not emit a structured payload.
pub async fn complete_review(
    pool: &PgPool,
    paper_id: Uuid,
    reviewer_id: Uuid,
    score: Option<f64>,
    recommendation: &str,
    comments: Option<&str>,
) -> Result<Option<ResearchPaper>> {
    complete_research_review(
        pool,
        paper_id,
        reviewer_id,
        score,
        recommendation,
        comments,
        None,
    )
    .await
}

/// Publishes an approved paper and awards its author Elo atomically.
///
/// The paper row is the idempotency key: it is locked for the duration of the
/// transaction, and `elo_awarded_at` is set only after the rating upsert.  A
/// retry therefore observes the marker and cannot increment the author's Elo
/// a second time.
pub async fn publish_and_award_elo(
    pool: &PgPool,
    paper_id: Uuid,
    elo_award: i32,
) -> Result<Option<ResearchPaper>> {
    let mut transaction = pool.begin().await?;

    let paper = sqlx::query_as::<_, (Uuid, Uuid, String, bool, Option<DateTime<Utc>>)>(
        "SELECT id, author_id, status, elo_awarded, elo_awarded_at\n         FROM research_papers\n         WHERE id = $1\n         FOR UPDATE",
    )
    .bind(paper_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some((paper_id, author_id, status, elo_awarded, _elo_awarded_at)) = paper else {
        transaction.commit().await?;
        return Ok(None);
    };

    if status == "approved" {
        sqlx::query(
            "UPDATE research_papers\n             SET status = 'published', published_at = COALESCE(published_at, CURRENT_TIMESTAMP)\n             WHERE id = $1 AND status = 'approved'",
        )
        .bind(paper_id)
        .execute(&mut *transaction)
        .await?;
    }

    if !matches!(status.as_str(), "approved" | "published") {
        transaction.commit().await?;
        return Ok(None);
    }

    if !elo_awarded {
        // `user_ratings` is the authoritative current-Elo table used by the
        // leaderboard.  Use the shared rating transaction so all award paths
        // apply the same baseline and delta semantics.
        rating_transaction::award_elo_for_source(&mut transaction, author_id, elo_award, paper_id)
            .await?;

        sqlx::query(
            "UPDATE research_papers\n             SET elo_award = $2, elo_awarded = TRUE, elo_awarded_at = CURRENT_TIMESTAMP\n             WHERE id = $1 AND NOT elo_awarded",
        )
        .bind(paper_id)
        .bind(elo_award)
        .execute(&mut *transaction)
        .await?;
    }

    let query = format!(
        "SELECT {} FROM research_papers WHERE id = $1",
        paper_columns()
    );
    let updated = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(&mut *transaction)
        .await?;

    transaction.commit().await?;
    Ok(updated)
}

pub async fn publish_paper_and_award_elo(
    pool: &PgPool,
    paper_id: Uuid,
    elo_award: i32,
) -> Result<Option<ResearchPaper>> {
    publish_and_award_elo(pool, paper_id, elo_award).await
}

// Kept local to avoid making the query module's projection a public SQL
// implementation detail.  It intentionally mirrors queries::research.
fn paper_columns() -> &'static str {
    "id, author_id, title, abstract AS abstract_text, content, status, submitted_at, under_review_at, decided_by, decided_at, published_at, evaluation_score::double precision AS evaluation_score, evaluation_result, elo_award, elo_awarded, elo_awarded_at, created_at, updated_at"
}
