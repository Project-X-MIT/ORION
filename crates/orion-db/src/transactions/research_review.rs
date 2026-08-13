use serde::Serialize;
use sqlx::{PgPool, Postgres, Result, Transaction};
use uuid::Uuid;

use crate::{models::ResearchPaper, transactions::write_outbox_event};

const RESEARCH_EVALUATION_RUBRIC_VERSION: u64 = 1;
const RESEARCH_NOTIFICATION_EVENT_TYPE: &str = "orion.notification.requested";

#[derive(Debug, Serialize)]
struct ResearchDecisionNotificationPayload {
    notification_id: Uuid,
    recipient_id: Uuid,
    kind: &'static str,
    title: String,
    body: String,
    action_url: String,
    deduplication_key: String,
}

/// Completes one review and moves the paper to `approved` or `rejected` in
/// the same transaction. The paper row is locked before the review is
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
        "SELECT id, author_id, status
         FROM research_papers
         WHERE id = $1
         FOR UPDATE",
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

    let Some(score_value) = score else {
        transaction.commit().await?;
        return Ok(None);
    };
    let Some(evaluation_result) = evaluation_result else {
        transaction.commit().await?;
        return Ok(None);
    };
    if comments.is_some_and(|value| value.trim().is_empty()) {
        transaction.commit().await?;
        return Ok(None);
    }

    // A paper author cannot review their own paper. The migration also
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
    if !valid_review_evaluation(score_value, next_status == "approved", evaluation_result) {
        transaction.commit().await?;
        return Ok(None);
    }

    let review_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO research_reviews
             (paper_id, reviewer_id, score, recommendation, comments, evaluation_result)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (paper_id, reviewer_id) DO UPDATE
         SET score = EXCLUDED.score,
             recommendation = EXCLUDED.recommendation,
             comments = EXCLUDED.comments,
             evaluation_result = EXCLUDED.evaluation_result,
             updated_at = CURRENT_TIMESTAMP
         RETURNING id",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .bind(score_value)
    .bind(recommendation)
    .bind(comments)
    .bind(evaluation_result)
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query(
        "UPDATE research_reviews
         SET reviewed_at = CURRENT_TIMESTAMP
         WHERE paper_id = $1 AND reviewer_id = $2",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .execute(&mut *transaction)
    .await?;

    // Keep the normalized aggregate on the paper for list/detail reads while
    // retaining every review row for auditability.
    let query = format!(
        "UPDATE research_papers
         SET status = $2,
             decided_by = $3,
             decided_at = COALESCE(decided_at, CURRENT_TIMESTAMP),
             evaluation_score = $4,
             evaluation_result = $5
         WHERE id = $1 AND status = 'under_review'
         RETURNING {}",
        paper_columns()
    );
    let updated = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(next_status)
        .bind(reviewer_id)
        .bind(score_value)
        .bind(evaluation_result)
        .fetch_optional(&mut *transaction)
        .await?;
    let Some(updated) = updated else {
        transaction.commit().await?;
        return Ok(None);
    };

    write_research_decision_notification(
        &mut transaction,
        paper_id,
        author_id,
        review_id,
        next_status == "approved",
    )
    .await?;

    transaction.commit().await?;
    Ok(Some(updated))
}

/// Applies an explicit approved/rejected decision through the DB-04
/// transaction boundary. It only accepts the latest matching reviewer record
/// when its structured evaluation is complete and internally consistent.
pub async fn decide_research_paper(
    pool: &PgPool,
    paper_id: Uuid,
    approved: bool,
) -> Result<Option<ResearchPaper>> {
    let mut transaction = pool.begin().await?;
    let paper = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT id, author_id, status
         FROM research_papers
         WHERE id = $1
         FOR UPDATE",
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

    let (recommendation_values, next_status) = if approved {
        ("'approve', 'approved'", "approved")
    } else {
        ("'reject', 'rejected'", "rejected")
    };
    let review_query = format!(
        "SELECT id, reviewer_id, score::double precision, evaluation_result
         FROM research_reviews
         WHERE paper_id = $1
           AND recommendation IN ({recommendation_values})
           AND score IS NOT NULL
           AND evaluation_result IS NOT NULL
         ORDER BY reviewed_at DESC, id DESC
         LIMIT 1"
    );
    let review = sqlx::query_as::<_, (Uuid, Uuid, f64, sqlx::types::JsonValue)>(&review_query)
        .bind(paper_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let Some((review_id, reviewer_id, score, evaluation_result)) = review else {
        transaction.commit().await?;
        return Ok(None);
    };
    if reviewer_id == author_id || !valid_review_evaluation(score, approved, &evaluation_result) {
        transaction.commit().await?;
        return Ok(None);
    }

    let query = format!(
        "UPDATE research_papers
         SET status = $2,
             decided_by = $3,
             decided_at = COALESCE(decided_at, CURRENT_TIMESTAMP),
             evaluation_score = $4,
             evaluation_result = $5
         WHERE id = $1 AND status = 'under_review'
         RETURNING {}",
        paper_columns()
    );
    let updated = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(next_status)
        .bind(reviewer_id)
        .bind(score)
        .bind(&evaluation_result)
        .fetch_optional(&mut *transaction)
        .await?;
    let Some(updated) = updated else {
        transaction.commit().await?;
        return Ok(None);
    };

    write_research_decision_notification(
        &mut transaction,
        paper_id,
        author_id,
        review_id,
        approved,
    )
    .await?;

    transaction.commit().await?;
    Ok(Some(updated))
}

async fn write_research_decision_notification(
    transaction: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    author_id: Uuid,
    review_id: Uuid,
    approved: bool,
) -> Result<Uuid> {
    let notification = ResearchDecisionNotificationPayload {
        notification_id: Uuid::new_v4(),
        recipient_id: author_id,
        kind: "research_decision",
        title: if approved {
            "Research paper approved".to_owned()
        } else {
            "Research paper rejected".to_owned()
        },
        body: if approved {
            "Your research paper was approved and is ready for publication.".to_owned()
        } else {
            "Your research paper was rejected after review.".to_owned()
        },
        action_url: format!("/api/v1/research/{paper_id}"),
        deduplication_key: format!(
            "research-review:{paper_id}:review:{review_id}:decision-notification"
        ),
    };
    write_outbox_event(transaction, RESEARCH_NOTIFICATION_EVENT_TYPE, notification).await
}

fn valid_review_evaluation(
    score: f64,
    approved: bool,
    evaluation: &sqlx::types::JsonValue,
) -> bool {
    if !score.is_finite() || !(0.0..=100.0).contains(&score) {
        return false;
    }
    let Some(object) = evaluation.as_object() else {
        return false;
    };
    if object
        .get("rubric_version")
        .and_then(|value| value.as_u64())
        != Some(RESEARCH_EVALUATION_RUBRIC_VERSION)
        || object
            .get("evaluated_content_version")
            .and_then(|value| value.as_u64())
            .is_none_or(|version| version == 0)
    {
        return false;
    }

    let Some(overall_score) = object.get("overall_score").and_then(|value| value.as_u64()) else {
        return false;
    };
    if overall_score > 100 || f64::from(overall_score as u32) != score {
        return false;
    }

    let expected_recommendation = if approved { "approve" } else { "reject" };
    if object
        .get("recommendation")
        .and_then(|value| value.as_str())
        .and_then(canonical_recommendation)
        != Some(expected_recommendation)
    {
        return false;
    }

    let Some(scores) = object.get("scores").and_then(|value| value.as_object()) else {
        return false;
    };
    let Some(relevance) = scores.get("relevance").and_then(|value| value.as_u64()) else {
        return false;
    };
    let Some(methodology) = scores.get("methodology").and_then(|value| value.as_u64()) else {
        return false;
    };
    let Some(evidence) = scores.get("evidence").and_then(|value| value.as_u64()) else {
        return false;
    };
    let Some(originality) = scores.get("originality").and_then(|value| value.as_u64()) else {
        return false;
    };
    let Some(clarity_and_reproducibility) = scores
        .get("clarity_and_reproducibility")
        .and_then(|value| value.as_u64())
    else {
        return false;
    };
    if [
        relevance,
        methodology,
        evidence,
        originality,
        clarity_and_reproducibility,
    ]
    .into_iter()
    .any(|value| value > 100)
    {
        return false;
    }
    let weighted_score = (relevance * 15
        + methodology * 25
        + evidence * 30
        + originality * 15
        + clarity_and_reproducibility * 15)
        / 100;
    if weighted_score != overall_score {
        return false;
    }

    let rationale_valid = object
        .get("rationale")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty());
    let evidence_valid = object
        .get("evidence")
        .and_then(|value| value.as_array())
        .is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    let Some(item) = item.as_object() else {
                        return false;
                    };
                    item.get("reference")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                        && item
                            .get("finding")
                            .and_then(|value| value.as_str())
                            .is_some_and(|value| !value.trim().is_empty())
                })
        });
    rationale_valid
        && evidence_valid
        && valid_feedback_array(object.get("strengths"))
        && valid_feedback_array(object.get("concerns"))
}

fn valid_feedback_array(value: Option<&sqlx::types::JsonValue>) -> bool {
    value
        .and_then(|value| value.as_array())
        .is_some_and(|items| {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| item.as_str().is_some_and(|value| !value.trim().is_empty()))
        })
}

fn canonical_recommendation(value: &str) -> Option<&'static str> {
    match value.trim() {
        "approve" | "approved" => Some("approve"),
        "reject" | "rejected" => Some("reject"),
        _ => None,
    }
}

/// Compatibility form for callers that provide the structured payload through
/// the transaction's evaluation argument. Decisions without that payload are
/// rejected rather than persisted.
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

// Kept local to avoid making the query module's projection a public SQL
// implementation detail. It intentionally mirrors queries::research.
fn paper_columns() -> &'static str {
    "id, author_id, title, abstract AS abstract_text, content, status, submitted_at, under_review_at, decided_by, decided_at, published_at, evaluation_score::double precision AS evaluation_score, evaluation_result, elo_award, elo_awarded, elo_awarded_at, created_at, updated_at"
}
