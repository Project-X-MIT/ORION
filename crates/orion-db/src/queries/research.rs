use chrono::{DateTime, Utc};
use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::models::{
    NewResearchPaper, NewResearchReview, ResearchPaper, ResearchPaperStatus, ResearchReview,
};

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
    let next_status = if approved {
        ResearchPaperStatus::Approved
    } else {
        ResearchPaperStatus::Rejected
    };
    let recommendation_values = if approved {
        "'approve', 'approved'"
    } else {
        "'reject', 'rejected'"
    };
    let query = format!(
        "UPDATE research_papers\n         SET status = $2,\n             decided_by = (\n                 SELECT reviewer_id\n                 FROM research_reviews\n                 WHERE paper_id = $1 AND recommendation IN ({recommendation_values})\n                 ORDER BY reviewed_at DESC, id DESC\n                 LIMIT 1\n             ),\n             decided_at = COALESCE(decided_at, CURRENT_TIMESTAMP)\n         WHERE id = $1\n           AND status = 'under_review'\n           AND EXISTS (\n               SELECT 1\n               FROM research_reviews\n               WHERE paper_id = $1 AND recommendation IN ({recommendation_values})\n           )\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .bind(next_status.as_str())
        .fetch_optional(pool)
        .await
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

/// Publishes an approved paper. Elo calculation and application are owned by
/// the Elo consumer after it receives Phantom's versioned evaluation request.
pub async fn publish_paper(pool: &PgPool, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
    let query = format!(
        "UPDATE research_papers\n         SET status = 'published', published_at = COALESCE(published_at, CURRENT_TIMESTAMP)\n         WHERE id = $1 AND status = 'approved'\n         RETURNING {}",
        RESEARCH_PAPER_COLUMNS
    );

    sqlx::query_as::<_, ResearchPaper>(&query)
        .bind(paper_id)
        .fetch_optional(pool)
        .await
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
        "UPDATE research_papers\n         SET evaluation_score = $2, evaluation_result = $3\n         WHERE id = $1 AND status IN ('under_review', 'approved')\n         RETURNING {}",
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
    let query = format!(
        "UPDATE research_reviews\n         SET score = $3,\n             evaluation_result = $4,\n             reviewed_at = CURRENT_TIMESTAMP\n         WHERE id = $1 AND reviewer_id = $2\n         RETURNING {}",
        RESEARCH_REVIEW_COLUMNS
    );

    sqlx::query_as::<_, ResearchReview>(&query)
        .bind(review_id)
        .bind(reviewer_id)
        .bind(score)
        .bind(evaluation_result)
        .fetch_optional(pool)
        .await
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
    let query = format!(
        "INSERT INTO research_reviews\n             (paper_id, reviewer_id, score, recommendation, comments, evaluation_result)\n         SELECT $1, $2, $3, $4, $5, $6\n         WHERE EXISTS (\n             SELECT 1 FROM research_papers\n             WHERE id = $1 AND status = 'under_review'\n         )\n         ON CONFLICT (paper_id, reviewer_id) DO UPDATE\n         SET score = EXCLUDED.score,\n             recommendation = EXCLUDED.recommendation,\n             comments = EXCLUDED.comments,\n             evaluation_result = EXCLUDED.evaluation_result,\n             reviewed_at = CURRENT_TIMESTAMP,\n             updated_at = CURRENT_TIMESTAMP\n         RETURNING {}",
        RESEARCH_REVIEW_COLUMNS
    );

    sqlx::query_as::<_, ResearchReview>(&query)
        .bind(review.paper_id)
        .bind(review.reviewer_id)
        .bind(review.score)
        .bind(review.recommendation)
        .bind(review.comments)
        .bind(review.evaluation_result)
        .fetch_one(pool)
        .await
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

/// Reads the award result marker for the Elo consumer's idempotency checks.
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
