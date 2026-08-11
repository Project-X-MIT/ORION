use anyhow::{anyhow, Context, Result};
use orion_db::{queries::research::research_award_idempotency_key, write_outbox_event};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const RESEARCH_ELO_AWARD_EVENT_TYPE: &str = "orion.research.elo_award.requested";
const RESEARCH_ELO_AWARD_SCHEMA_VERSION: u16 = 1;

/// Phantom's persisted request body for the versioned Yash handoff.
///
/// This contains the validated research facts only. It deliberately contains
/// no Elo delta; Yash owns the score-to-Elo policy and calculation.
#[derive(Debug, Serialize)]
struct ResearchEloAwardRequestPayload {
    paper_id: Uuid,
    author_id: Uuid,
    paper_status: &'static str,
    rubric_version: u16,
    evaluated_content_version: u32,
    evaluation_score: u8,
    recommendation: &'static str,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
struct PersistedEvaluation {
    rubric_version: u16,
    evaluated_content_version: u32,
    scores: PersistedRubricScores,
    overall_score: u8,
    recommendation: String,
    rationale: String,
    evidence: Vec<PersistedEvidence>,
    strengths: Vec<String>,
    concerns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PersistedRubricScores {
    relevance: u8,
    methodology: u8,
    evidence: u8,
    originality: u8,
    clarity_and_reproducibility: u8,
}

#[derive(Debug, Deserialize)]
struct PersistedEvidence {
    reference: String,
    finding: String,
}

impl PersistedEvaluation {
    fn is_valid_for(&self, score: f64, recommendation: &str) -> bool {
        if self.rubric_version != RESEARCH_ELO_AWARD_SCHEMA_VERSION
            || self.evaluated_content_version == 0
            || self.overall_score > 100
            || !score.is_finite()
            || score != f64::from(self.overall_score)
            || !matches!(self.recommendation.as_str(), "approve" | "approved")
            || !matches!(recommendation, "approve" | "approved")
            || self.scores.relevance > 100
            || self.scores.methodology > 100
            || self.scores.evidence > 100
            || self.scores.originality > 100
            || self.scores.clarity_and_reproducibility > 100
            || self.scores.weighted_score() != self.overall_score
            || self.rationale.trim().is_empty()
            || self.evidence.is_empty()
            || self
                .evidence
                .iter()
                .any(|item| item.reference.trim().is_empty() || item.finding.trim().is_empty())
            || !valid_feedback(&self.strengths)
            || !valid_feedback(&self.concerns)
        {
            return false;
        }
        true
    }
}

impl PersistedRubricScores {
    fn weighted_score(&self) -> u8 {
        ((u32::from(self.relevance) * 15
            + u32::from(self.methodology) * 25
            + u32::from(self.evidence) * 30
            + u32::from(self.originality) * 15
            + u32::from(self.clarity_and_reproducibility) * 15)
            / 100) as u8
    }
}

fn valid_feedback(items: &[String]) -> bool {
    !items.is_empty() && items.iter().all(|item| !item.trim().is_empty())
}

/// Enqueues one eligible published research paper for Yash's Elo consumer.
///
/// The paper row is locked before checking the approved review and inserting
/// the outbox event. This makes concurrent retries produce at most one
/// request without calculating or applying Elo in Phantom's worker.
pub async fn enqueue_research_award(pool: &PgPool, paper_id: Uuid) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let enqueued = enqueue_research_award_in_transaction(&mut transaction, paper_id).await?;
    transaction.commit().await?;
    Ok(enqueued)
}

/// Composable form for callers that already own the business transaction.
pub async fn enqueue_research_award_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<bool> {
    let paper = sqlx::query_as::<_, (Uuid, String, bool, Option<Uuid>)>(
        "SELECT author_id, status, elo_awarded, decided_by
         FROM research_papers
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(paper_id)
    .fetch_optional(&mut **transaction)
    .await?;

    let Some((author_id, status, already_awarded, decided_by)) = paper else {
        return Ok(false);
    };
    if already_awarded || status != "published" {
        return Ok(false);
    }

    let Some(reviewer_id) = decided_by else {
        return Ok(false);
    };
    if reviewer_id == author_id {
        return Ok(false);
    }

    let review = sqlx::query_as::<_, (Uuid, f64, String, Value)>(
        "SELECT id, score, recommendation, evaluation_result
         FROM research_reviews
         WHERE paper_id = $1
           AND reviewer_id = $2
           AND recommendation IN ('approve', 'approved')
           AND score IS NOT NULL
           AND evaluation_result IS NOT NULL
         ORDER BY reviewed_at DESC, id DESC
         LIMIT 1",
    )
    .bind(paper_id)
    .bind(reviewer_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let Some((review_id, score, recommendation, evaluation_result)) = review else {
        return Ok(false);
    };

    let evaluation: PersistedEvaluation = serde_json::from_value(evaluation_result)
        .context("persisted research evaluation cannot be used for Elo handoff")?;
    if recommendation != "approve" && recommendation != "approved" {
        return Err(anyhow!(
            "persisted research evaluation is not a valid approved Elo handoff"
        ));
    }
    if !evaluation.is_valid_for(score, &recommendation) {
        return Err(anyhow!(
            "persisted research evaluation is not a valid approved Elo handoff"
        ));
    }

    let idempotency_key = research_award_idempotency_key(paper_id, review_id);
    let already_queued: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM outbox_events
             WHERE event_type = $1
               AND payload ->> 'paper_id' = $2
               AND payload ->> 'idempotency_key' = $3
         )",
    )
    .bind(RESEARCH_ELO_AWARD_EVENT_TYPE)
    .bind(paper_id.to_string())
    .bind(&idempotency_key)
    .fetch_one(&mut **transaction)
    .await?;
    if already_queued {
        return Ok(false);
    }

    let request = ResearchEloAwardRequestPayload {
        paper_id,
        author_id,
        paper_status: "published",
        rubric_version: evaluation.rubric_version,
        evaluated_content_version: evaluation.evaluated_content_version,
        evaluation_score: evaluation.overall_score,
        recommendation: "approve",
        idempotency_key,
    };
    write_outbox_event(transaction, RESEARCH_ELO_AWARD_EVENT_TYPE, request).await?;
    Ok(true)
}

/// Compatibility entrypoint for the research award job. The job now queues
/// the request; it does not calculate or apply an Elo award.
pub async fn process_research_award(pool: &PgPool, paper_id: Uuid) -> Result<bool> {
    enqueue_research_award(pool, paper_id).await
}

#[cfg(test)]
mod tests {
    use super::RESEARCH_ELO_AWARD_EVENT_TYPE;

    #[test]
    fn uses_the_versioned_research_request_event_type() {
        assert_eq!(
            RESEARCH_ELO_AWARD_EVENT_TYPE,
            "orion.research.elo_award.requested"
        );
    }
}
