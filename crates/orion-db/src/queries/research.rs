use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Result, Transaction};
use uuid::Uuid;

use crate::models::{
    NewResearchPaper, NewResearchReview, ResearchPaper, ResearchPaperStatus, ResearchReview,
};
use crate::transactions::{research_review, write_outbox_event};

const RESEARCH_ELO_AWARD_EVENT_TYPE: &str = "orion.research.elo_award.requested";
const RESEARCH_ELO_AWARD_CONTRACT_VERSION: i32 = 1;
const RESEARCH_RUBRIC_VERSION: i32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ResearchAwardEnqueueError {
    #[error("research award persistence failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("persisted research evaluation is not valid for an Elo handoff: {0}")]
    InvalidEvaluation(#[from] serde_json::Error),
    #[error("persisted research evaluation is not valid for an Elo handoff")]
    InvalidEvaluationData,
}

#[must_use]
pub fn research_award_idempotency_key(paper_id: Uuid, review_id: Uuid) -> String {
    format!("research-paper:{paper_id}:review:{review_id}:elo-award")
}

#[derive(Debug, Serialize)]
struct ResearchEloAwardRequestPayload {
    contract_version: i32,
    paper_id: Uuid,
    review_id: Uuid,
    author_id: Uuid,
    paper_status: &'static str,
    rubric_version: i32,
    evaluated_content_version: i32,
    evaluation_score: i32,
    recommendation: &'static str,
    idempotency_key: String,
}

#[derive(Debug, serde::Deserialize)]
struct PersistedEvaluation {
    rubric_version: i32,
    evaluated_content_version: i32,
    scores: PersistedRubricScores,
    overall_score: u8,
    recommendation: String,
    rationale: String,
    evidence: Vec<PersistedEvidence>,
    strengths: Vec<String>,
    concerns: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PersistedRubricScores {
    relevance: u8,
    methodology: u8,
    evidence: u8,
    originality: u8,
    clarity_and_reproducibility: u8,
}

#[derive(Debug, serde::Deserialize)]
struct PersistedEvidence {
    reference: String,
    finding: String,
}

impl PersistedEvaluation {
    fn is_valid_for(&self, score: f64, recommendation: &str) -> bool {
        if self.rubric_version != RESEARCH_RUBRIC_VERSION
            || self.evaluated_content_version <= 0
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

const RESEARCH_PAPER_COLUMNS: &str = r#"
    id,
    author_id,
    title,
    abstract AS abstract_text,
    content,
    status,
    submitted_at,
    under_review_at,
    decided_by,
    decided_at,
    published_at,
    evaluation_score::double precision AS evaluation_score,
    evaluation_result,
    elo_award,
    elo_awarded,
    elo_awarded_at,
    created_at,
    updated_at
"#;

const RESEARCH_REVIEW_COLUMNS: &str = r#"
    id,
    paper_id,
    reviewer_id,
    score::double precision AS score,
    recommendation,
    comments,
    evaluation_result,
    reviewed_at,
    created_at,
    updated_at
"#;

/// Creates a paper directly in `draft`.
pub async fn create_paper(
    pool: &PgPool,
    author_id: Uuid,
    title: &str,
    abstract_text: &str,
    content: &str,
) -> Result<ResearchPaper> {
    create_paper_from_input(
        pool,
        NewResearchPaper {
            author_id,
            title,
            abstract_text,
            content,
        },
    )
    .await
}

/// Creates a new research paper explicitly in the `draft` state.
pub async fn create_draft(
    pool: &PgPool,
    author_id: Uuid,
    title: &str,
    abstract_text: &str,
    content: &str,
) -> Result<ResearchPaper> {
    create_paper(pool, author_id, title, abstract_text, content).await
}

pub async fn create_paper_from_input(
    pool: &PgPool,
    paper: NewResearchPaper<'_>,
) -> Result<ResearchPaper> {
    let query = format!(
        "INSERT INTO research_papers (author_id, title, abstract, content)\n         VALUES ($1, $2, $3, $4)\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper.author_id)
        .bind(paper.title)
        .bind(paper.abstract_text)
        .bind(paper.content)
        .fetch_one(pool)
        .await
}

/// Creates a new draft identity for a post-decision re-review. The source row
/// is never reopened or mutated; callers must provide a stable new UUID so a
/// retried request keeps the same version identity.
pub async fn create_revision(
    pool: &PgPool,
    source_paper_id: Uuid,
    requester_id: Uuid,
    new_paper_id: Uuid,
    title: &str,
    abstract_text: &str,
    content: &str,
) -> Result<Option<ResearchPaper>> {
    if new_paper_id.is_nil() || new_paper_id == source_paper_id {
        return Ok(None);
    }

    let mut transaction = pool.begin().await?;
    let source = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT author_id, status
         FROM research_papers
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(source_paper_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((author_id, status)) = source else {
        transaction.commit().await?;
        return Ok(None);
    };
    if author_id != requester_id
        || !matches!(status.as_str(), "approved" | "rejected" | "published")
    {
        transaction.commit().await?;
        return Ok(None);
    }

    let query = format!(
        "INSERT INTO research_papers
             (id, author_id, title, abstract, content)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO NOTHING
         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );
    let mut revision = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(new_paper_id)
        .bind(author_id)
        .bind(title)
        .bind(abstract_text)
        .bind(content)
        .fetch_optional(&mut *transaction)
        .await?;

    if revision.is_none() {
        let existing_query = format!(
            "SELECT {} FROM research_papers
             WHERE id = $1
               AND author_id = $2
               AND title = $3
               AND abstract = $4
               AND content = $5
             FOR UPDATE",
            RESEARCH_PAPER_COLUMNS
        );
        revision = sqlx::query_as::<_, ResearchPaper>(&existing_query)
            .bind(new_paper_id)
            .bind(author_id)
            .bind(title)
            .bind(abstract_text)
            .bind(content)
            .fetch_optional(&mut *transaction)
            .await?;
    }

    transaction.commit().await?;
    Ok(revision)
}

/// Returns a paper by its immutable identifier.
pub async fn find_by_id(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers WHERE id = $1",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(pool)
        .await
}

pub async fn find_paper_by_id(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    find_by_id(pool, paper_id).await
}

pub async fn find_draft_by_id(
    pool: &PgPool,
    paper_id: Uuid,
    author_id: Uuid,
) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE id = $1 AND author_id = $2 AND status = 'draft'",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(author_id)
        .fetch_optional(pool)
        .await
}

/// Returns a page of an author's papers, newest first.
pub async fn list_by_author_id(
    pool: &PgPool,
    author_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE author_id = $1\n         ORDER BY created_at DESC, id DESC\n         LIMIT $2 OFFSET $3",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(author_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn papers_by_author_id(
    pool: &PgPool,
    author_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    list_by_author_id(pool, author_id, limit, offset).await
}

pub async fn research_by_author(
    pool: &PgPool,
    author_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    list_by_author_id(pool, author_id, limit, offset).await
}

/// Returns only editable drafts owned by an author, newest first.
pub async fn list_drafts_by_author_id(
    pool: &PgPool,
    author_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE author_id = $1 AND status = 'draft'\n         ORDER BY created_at DESC, id DESC\n         LIMIT $2 OFFSET $3",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(author_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

/// Returns papers waiting for a reviewer.  `under_review` is included because
/// workers may claim a submitted paper just before the next polling cycle.
pub async fn list_for_review(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE status IN ('submitted', 'under_review')\n         ORDER BY submitted_at ASC NULLS LAST, id ASC\n         LIMIT $1 OFFSET $2",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn pending_reviews(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
    list_for_review(pool, limit, offset).await
}

pub async fn list_pending_reviews(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    pending_reviews(pool, limit, offset).await
}

pub async fn pending_review_count(pool: &PgPool) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint\n         FROM research_papers\n         WHERE status IN ('submitted', 'under_review')",
    )
    .fetch_one(pool)
    .await
}

pub async fn submitted_papers(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE status = 'submitted'\n         ORDER BY submitted_at ASC NULLS LAST, id ASC\n         LIMIT $1 OFFSET $2",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn list_published(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE status = 'published'\n         ORDER BY published_at DESC, id DESC\n         LIMIT $1 OFFSET $2",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}

pub async fn published_papers(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    list_published(pool, limit, offset).await
}

pub async fn published_research(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<ResearchPaper>> {
    list_published(pool, limit, offset).await
}

pub async fn find_published_by_id(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "SELECT {} FROM research_papers\n         WHERE id = $1 AND status = 'published'",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(pool)
        .await
}

/// Updates editable draft fields.  Submitted or reviewed papers cannot be
/// silently rewritten by an author.
pub async fn update_draft(
    pool: &PgPool,
    paper_id: Uuid,
    author_id: Uuid,
    title: &str,
    abstract_text: &str,
    content: &str,
) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "UPDATE research_papers\n         SET title = $3, abstract = $4, content = $5\n         WHERE id = $1 AND author_id = $2 AND status = 'draft'\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(author_id)
        .bind(title)
        .bind(abstract_text)
        .bind(content)
        .fetch_optional(pool)
        .await
}

/// Moves a draft to `submitted`.  The conditional update makes duplicate
/// submissions harmless and prevents a stale writer from skipping a state.
pub async fn submit_paper(
    pool: &PgPool,
    paper_id: Uuid,
    author_id: Uuid,
) -> Result<Option<ResearchPaper>> {
    transition_paper(
        pool,
        paper_id,
        Some(author_id),
        ResearchPaperStatus::Draft,
        ResearchPaperStatus::Submitted,
        "submitted_at",
    )
    .await
}

pub async fn submit_for_review(
    pool: &PgPool,
    paper_id: Uuid,
    author_id: Uuid,
) -> Result<Option<ResearchPaper>> {
    submit_paper(pool, paper_id, author_id).await
}

/// Claims a submitted paper for review.
pub async fn mark_under_review(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    transition_paper(
        pool,
        paper_id,
        None,
        ResearchPaperStatus::Submitted,
        ResearchPaperStatus::UnderReview,
        "under_review_at",
    )
    .await
}

pub async fn begin_review(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    mark_under_review(pool, paper_id).await
}

async fn transition_paper(
    pool: &PgPool,
    paper_id: Uuid,
    author_id: Option<Uuid>,
    from: ResearchPaperStatus,
    to: ResearchPaperStatus,
    timestamp_column: &str,
) -> Result<Option<ResearchPaper>> {
    let has_author = author_id.is_some();
    let author_clause = if has_author {
        " AND author_id = $2"
    } else {
        ""
    };
    let to_placeholder = if has_author { "$3" } else { "$2" };
    let from_placeholder = if has_author { "$4" } else { "$3" };
    let query = format!(
        "UPDATE research_papers\n         SET status = {to_placeholder}, {timestamp_column} = COALESCE({timestamp_column}, CURRENT_TIMESTAMP)\n         WHERE id = $1{author_clause} AND status = {from_placeholder}\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    let mut request = sqlx::query_as::<_, ResearchPaper>(&query).bind(paper_id);
    if let Some(author_id) = author_id {
        request = request.bind(author_id);
    }
    request
        .bind(to.as_str())
        .bind(from.as_str())
        .fetch_optional(pool)
        .await
}

/// Explicitly records the review decision without publishing the paper.
pub async fn decide_paper(
    pool: &PgPool,
    paper_id: Uuid,
    approved: bool,
) -> Result<Option<ResearchPaper>> {
    research_review::decide_research_paper(pool, paper_id, approved).await
}

pub async fn approve_paper(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    decide_paper(pool, paper_id, true).await
}

pub async fn approve(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    approve_paper(pool, paper_id).await
}

pub async fn reject_paper(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    decide_paper(pool, paper_id, false).await
}

pub async fn reject(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    reject_paper(pool, paper_id).await
}

/// Enqueues one eligible published paper for Yash's Elo consumer.
///
/// Eligibility, persisted-evaluation validation, idempotency, and the outbox
/// write are owned by this database transaction. Worker callers only invoke
/// this operation and manage delivery state around it.
pub async fn enqueue_research_award(
    pool: &PgPool,
    paper_id: Uuid,
) -> std::result::Result<bool, ResearchAwardEnqueueError> {
    let mut transaction = pool.begin().await?;
    let enqueued = enqueue_research_award_in_transaction(&mut transaction, paper_id).await?;
    transaction.commit().await?;
    Ok(enqueued)
}

/// Composable form for callers that already own the business transaction.
pub async fn enqueue_research_award_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> std::result::Result<bool, ResearchAwardEnqueueError> {
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

    let review = sqlx::query_as::<_, (Uuid, f64, String, sqlx::types::JsonValue)>(
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

    let evaluation: PersistedEvaluation = serde_json::from_value(evaluation_result)?;
    if !evaluation.is_valid_for(score, &recommendation) {
        return Err(ResearchAwardEnqueueError::InvalidEvaluationData);
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
        contract_version: RESEARCH_ELO_AWARD_CONTRACT_VERSION,
        paper_id,
        review_id,
        author_id,
        paper_status: "published",
        rubric_version: evaluation.rubric_version,
        evaluated_content_version: evaluation.evaluated_content_version,
        evaluation_score: i32::from(evaluation.overall_score),
        recommendation: "approve",
        idempotency_key,
    };
    write_outbox_event(transaction, RESEARCH_ELO_AWARD_EVENT_TYPE, request).await?;
    Ok(true)
}

/// Publishes an approved paper and queues Phantom's versioned Elo request in
/// the same transaction. If the outbox write fails, publication rolls back;
/// the downstream Elo contract remains owned by Yash.
pub async fn publish_paper(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    let mut transaction = pool.begin().await?;
    let published = publish_paper_in_transaction(&mut transaction, paper_id).await?;
    transaction.commit().await?;
    Ok(published)
}

async fn publish_paper_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Option<ResearchPaper>> {
    let eligible = sqlx::query_as::<_, (Uuid, Uuid, f64, i32, i32, i32)>(
        "SELECT p.author_id,
                r.id,
                r.score::double precision,
                (r.evaluation_result ->> 'rubric_version')::integer,
                (r.evaluation_result ->> 'evaluated_content_version')::integer,
                (r.evaluation_result ->> 'overall_score')::integer
         FROM research_papers AS p
         JOIN research_reviews AS r
           ON r.paper_id = p.id AND r.reviewer_id = p.decided_by
         WHERE p.id = $1
           AND p.status = 'approved'
           AND r.recommendation IN ('approve', 'approved')
           AND r.score IS NOT NULL
           AND r.evaluation_result IS NOT NULL
           AND r.evaluation_result ->> 'recommendation' IN ('approve', 'approved')
           AND (r.evaluation_result ->> 'rubric_version')::integer = $2
           AND (r.evaluation_result ->> 'evaluated_content_version')::integer > 0
           AND r.score = (r.evaluation_result ->> 'overall_score')::double precision
         ORDER BY r.reviewed_at DESC, r.id DESC
         LIMIT 1
         FOR UPDATE OF p",
    )
    .bind(paper_id)
    .bind(RESEARCH_RUBRIC_VERSION)
    .fetch_optional(&mut **transaction)
    .await?;

    let Some((author_id, review_id, _score, rubric_version, evaluated_content_version, score)) =
        eligible
    else {
        return Ok(None);
    };

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

    let query = format!(
        "UPDATE research_papers\n         SET status = 'published', published_at = COALESCE(published_at, CURRENT_TIMESTAMP)\n         WHERE id = $1 AND status = 'approved'\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );
    let published = sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(&mut **transaction)
        .await?;
    let Some(published) = published else {
        return Ok(None);
    };

    if !already_queued {
        let request = ResearchEloAwardRequestPayload {
            contract_version: RESEARCH_ELO_AWARD_CONTRACT_VERSION,
            paper_id,
            review_id,
            author_id,
            paper_status: "published",
            rubric_version,
            evaluated_content_version,
            evaluation_score: score,
            recommendation: "approve",
            idempotency_key,
        };
        write_outbox_event(transaction, RESEARCH_ELO_AWARD_EVENT_TYPE, request).await?;
    }

    Ok(Some(published))
}

pub async fn publish(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    publish_paper(pool, paper_id).await
}

/// Adds or replaces the structured result associated with a paper evaluation.
pub async fn record_evaluation_result(
    pool: &PgPool,
    paper_id: Uuid,
    evaluation_score: Option<f64>,
    evaluation_result: Option<&sqlx::types::JsonValue>,
) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "UPDATE research_papers\n         SET evaluation_score = $2, evaluation_result = $3\n         WHERE id = $1 AND status = 'under_review'\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(evaluation_score)
        .bind(evaluation_result)
        .fetch_optional(pool)
        .await
}

pub async fn list_reviews_by_paper_id(
    pool: &PgPool,
    paper_id: Uuid,
) -> Result<Vec<ResearchReview>> {
    let query = format!(
        "SELECT {} FROM research_reviews\n         WHERE paper_id = $1\n         ORDER BY created_at ASC, id ASC",
        RESEARCH_REVIEW_COLUMNS
    );

    sqlx::query_as::<_, ResearchReview>(&query)
        .bind(paper_id)
        .fetch_all(pool)
        .await
}

pub async fn reviews_by_paper_id(pool: &PgPool, paper_id: Uuid) -> Result<Vec<ResearchReview>> {
    list_reviews_by_paper_id(pool, paper_id).await
}

pub async fn find_review_by_id(pool: &PgPool, review_id: Uuid) -> Result<Option<ResearchReview>> {
    let query = format!(
        "SELECT {} FROM research_reviews WHERE id = $1",
        RESEARCH_REVIEW_COLUMNS
    );

    sqlx::query_as::<_, ResearchReview>(&query)
        .bind(review_id)
        .fetch_optional(pool)
        .await
}

pub async fn store_review_evaluation(
    pool: &PgPool,
    review_id: Uuid,
    reviewer_id: Uuid,
    score: Option<f64>,
    evaluation_result: Option<&sqlx::types::JsonValue>,
) -> Result<Option<ResearchReview>> {
    let mut transaction = pool.begin().await?;
    let review_context = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT r.paper_id, p.status
         FROM research_reviews AS r
         JOIN research_papers AS p ON p.id = r.paper_id
         WHERE r.id = $1 AND r.reviewer_id = $2
         FOR UPDATE OF r, p",
    )
    .bind(review_id)
    .bind(reviewer_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((_paper_id, status)) = review_context else {
        transaction.commit().await?;
        return Ok(None);
    };
    if status != "under_review" {
        transaction.commit().await?;
        return Ok(None);
    }

    let query = format!(
        "UPDATE research_reviews\n         SET score = $3,\n             evaluation_result = $4,\n             reviewed_at = CURRENT_TIMESTAMP\n         WHERE id = $1 AND reviewer_id = $2\n           AND EXISTS (\n               SELECT 1\n               FROM research_papers\n               WHERE research_papers.id = research_reviews.paper_id\n                 AND research_papers.status = 'under_review'\n           )\n         RETURNING {}",
        RESEARCH_REVIEW_COLUMNS
    );

    let updated = sqlx::query_as::<_, ResearchReview>(&query)
        .bind(review_id)
        .bind(reviewer_id)
        .bind(score)
        .bind(evaluation_result)
        .fetch_optional(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(updated)
}

pub async fn update_review_evaluation(
    pool: &PgPool,
    review_id: Uuid,
    reviewer_id: Uuid,
    score: Option<f64>,
    evaluation_result: Option<&sqlx::types::JsonValue>,
) -> Result<Option<ResearchReview>> {
    store_review_evaluation(pool, review_id, reviewer_id, score, evaluation_result).await
}

/// Persists a review outside the decision transaction.  The unique
/// `(paper_id, reviewer_id)` constraint makes worker retries safe.
pub async fn create_review(pool: &PgPool, review: NewResearchReview<'_>) -> Result<ResearchReview> {
    let mut transaction = pool.begin().await?;
    let paper_status: Option<String> = sqlx::query_scalar(
        "SELECT status
         FROM research_papers
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(review.paper_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if paper_status.as_deref() != Some("under_review") {
        transaction.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    let query = format!(
        "INSERT INTO research_reviews\n             (paper_id, reviewer_id, score, recommendation, comments, evaluation_result)\n         SELECT $1, $2, $3, $4, $5, $6\n         WHERE EXISTS (\n             SELECT 1 FROM research_papers\n             WHERE id = $1 AND status = 'under_review'\n         )\n         ON CONFLICT (paper_id, reviewer_id) DO UPDATE\n         SET score = EXCLUDED.score,\n             recommendation = EXCLUDED.recommendation,\n             comments = EXCLUDED.comments,\n             evaluation_result = EXCLUDED.evaluation_result,\n             reviewed_at = CURRENT_TIMESTAMP,\n             updated_at = CURRENT_TIMESTAMP\n         RETURNING {}",
        RESEARCH_REVIEW_COLUMNS
    );

    let persisted = sqlx::query_as::<_, ResearchReview>(&query)
        .bind(review.paper_id)
        .bind(review.reviewer_id)
        .bind(review.score)
        .bind(review.recommendation)
        .bind(review.comments)
        .bind(review.evaluation_result)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(persisted)
}

pub async fn insert_review(
    pool: &PgPool,
    paper_id: Uuid,
    reviewer_id: Uuid,
    score: Option<f64>,
    recommendation: &str,
    comments: Option<&str>,
    evaluation_result: Option<&sqlx::types::JsonValue>,
) -> Result<ResearchReview> {
    create_review(
        pool,
        NewResearchReview {
            paper_id,
            reviewer_id,
            score,
            recommendation,
            comments,
            evaluation_result,
        },
    )
    .await
}

/// Reads the current award marker.  This is useful to workers deciding
/// whether an interrupted publication transaction needs to be retried.
pub async fn elo_award_state(
    pool: &PgPool,
    paper_id: Uuid,
) -> Result<Option<(Option<i32>, Option<DateTime<Utc>>)>> {
    sqlx::query_as("SELECT elo_award, elo_awarded_at\n         FROM research_papers WHERE id = $1")
        .bind(paper_id)
        .fetch_optional(pool)
        .await
}

pub async fn elo_awarded(pool: &PgPool, paper_id: Uuid) -> Result<Option<bool>> {
    sqlx::query_scalar::<_, bool>(
        "SELECT elo_awarded\n         FROM research_papers\n         WHERE id = $1",
    )
    .bind(paper_id)
    .fetch_optional(pool)
    .await
}
