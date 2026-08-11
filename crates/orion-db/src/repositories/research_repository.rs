use sqlx::{PgPool, Result};
use uuid::Uuid;

use crate::{
    models::{NewResearchPaper, NewResearchReview, ResearchPaper, ResearchReview},
    queries::research,
    transactions::research_review,
};

/// Persistence gateway for authoring, reviewing, and publishing research.
#[derive(Debug, Clone)]
pub struct ResearchRepository {
    pool: PgPool,
}

impl ResearchRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_paper(
        &self,
        author_id: Uuid,
        title: &str,
        abstract_text: &str,
        content: &str,
    ) -> Result<ResearchPaper> {
        research::create_paper(&self.pool, author_id, title, abstract_text, content).await
    }

    pub async fn create_draft(
        &self,
        author_id: Uuid,
        title: &str,
        abstract_text: &str,
        content: &str,
    ) -> Result<ResearchPaper> {
        research::create_draft(&self.pool, author_id, title, abstract_text, content).await
    }

    pub async fn create_paper_from_input(
        &self,
        paper: NewResearchPaper<'_>,
    ) -> Result<ResearchPaper> {
        research::create_paper_from_input(&self.pool, paper).await
    }

    pub async fn create_revision(
        &self,
        source_paper_id: Uuid,
        requester_id: Uuid,
        new_paper_id: Uuid,
        title: &str,
        abstract_text: &str,
        content: &str,
    ) -> Result<Option<ResearchPaper>> {
        research::create_revision(
            &self.pool,
            source_paper_id,
            requester_id,
            new_paper_id,
            title,
            abstract_text,
            content,
        )
        .await
    }

    pub async fn find_by_id(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::find_by_id(&self.pool, paper_id).await
    }

    pub async fn find_paper_by_id(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        self.find_by_id(paper_id).await
    }

    pub async fn find_draft_by_id(
        &self,
        paper_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<ResearchPaper>> {
        research::find_draft_by_id(&self.pool, paper_id, author_id).await
    }

    pub async fn list_by_author_id(
        &self,
        author_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResearchPaper>> {
        research::list_by_author_id(&self.pool, author_id, limit, offset).await
    }

    pub async fn papers_by_author_id(
        &self,
        author_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResearchPaper>> {
        self.list_by_author_id(author_id, limit, offset).await
    }

    pub async fn research_by_author(
        &self,
        author_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResearchPaper>> {
        research::research_by_author(&self.pool, author_id, limit, offset).await
    }

    pub async fn list_drafts_by_author_id(
        &self,
        author_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResearchPaper>> {
        research::list_drafts_by_author_id(&self.pool, author_id, limit, offset).await
    }

    pub async fn list_for_review(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        research::list_for_review(&self.pool, limit, offset).await
    }

    pub async fn pending_reviews(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        research::pending_reviews(&self.pool, limit, offset).await
    }

    pub async fn list_pending_reviews(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ResearchPaper>> {
        self.pending_reviews(limit, offset).await
    }

    pub async fn pending_review_count(&self) -> Result<i64> {
        research::pending_review_count(&self.pool).await
    }

    pub async fn list_published(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        research::list_published(&self.pool, limit, offset).await
    }

    pub async fn published_research(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        research::published_research(&self.pool, limit, offset).await
    }

    pub async fn find_published_by_id(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::find_published_by_id(&self.pool, paper_id).await
    }

    pub async fn submitted_papers(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        research::submitted_papers(&self.pool, limit, offset).await
    }

    pub async fn published_papers(&self, limit: i64, offset: i64) -> Result<Vec<ResearchPaper>> {
        self.list_published(limit, offset).await
    }

    pub async fn update_draft(
        &self,
        paper_id: Uuid,
        author_id: Uuid,
        title: &str,
        abstract_text: &str,
        content: &str,
    ) -> Result<Option<ResearchPaper>> {
        research::update_draft(
            &self.pool,
            paper_id,
            author_id,
            title,
            abstract_text,
            content,
        )
        .await
    }

    pub async fn submit_paper(
        &self,
        paper_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<ResearchPaper>> {
        research::submit_paper(&self.pool, paper_id, author_id).await
    }

    pub async fn submit_for_review(
        &self,
        paper_id: Uuid,
        author_id: Uuid,
    ) -> Result<Option<ResearchPaper>> {
        research::submit_for_review(&self.pool, paper_id, author_id).await
    }

    pub async fn mark_under_review(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::mark_under_review(&self.pool, paper_id).await
    }

    pub async fn begin_review(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        self.mark_under_review(paper_id).await
    }

    pub async fn decide_paper(
        &self,
        paper_id: Uuid,
        approved: bool,
    ) -> Result<Option<ResearchPaper>> {
        research::decide_paper(&self.pool, paper_id, approved).await
    }

    pub async fn approve_paper(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::approve_paper(&self.pool, paper_id).await
    }

    pub async fn approve(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        self.approve_paper(paper_id).await
    }

    pub async fn reject_paper(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::reject_paper(&self.pool, paper_id).await
    }

    pub async fn reject(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        self.reject_paper(paper_id).await
    }

    pub async fn publish_paper(&self, paper_id: Uuid) -> Result<Option<ResearchPaper>> {
        research::publish_paper(&self.pool, paper_id).await
    }

    pub async fn record_evaluation_result(
        &self,
        paper_id: Uuid,
        score: Option<f64>,
        result: Option<&sqlx::types::JsonValue>,
    ) -> Result<Option<ResearchPaper>> {
        research::record_evaluation_result(&self.pool, paper_id, score, result).await
    }

    pub async fn list_reviews_by_paper_id(&self, paper_id: Uuid) -> Result<Vec<ResearchReview>> {
        research::list_reviews_by_paper_id(&self.pool, paper_id).await
    }

    pub async fn reviews_by_paper_id(&self, paper_id: Uuid) -> Result<Vec<ResearchReview>> {
        self.list_reviews_by_paper_id(paper_id).await
    }

    pub async fn find_review_by_id(&self, review_id: Uuid) -> Result<Option<ResearchReview>> {
        research::find_review_by_id(&self.pool, review_id).await
    }

    pub async fn store_review_evaluation(
        &self,
        review_id: Uuid,
        reviewer_id: Uuid,
        score: Option<f64>,
        evaluation_result: Option<&sqlx::types::JsonValue>,
    ) -> Result<Option<ResearchReview>> {
        research::store_review_evaluation(
            &self.pool,
            review_id,
            reviewer_id,
            score,
            evaluation_result,
        )
        .await
    }

    pub async fn update_review_evaluation(
        &self,
        review_id: Uuid,
        reviewer_id: Uuid,
        score: Option<f64>,
        evaluation_result: Option<&sqlx::types::JsonValue>,
    ) -> Result<Option<ResearchReview>> {
        self.store_review_evaluation(review_id, reviewer_id, score, evaluation_result)
            .await
    }

    pub async fn create_review(&self, review: NewResearchReview<'_>) -> Result<ResearchReview> {
        research::create_review(&self.pool, review).await
    }

    pub async fn insert_review(
        &self,
        paper_id: Uuid,
        reviewer_id: Uuid,
        score: Option<f64>,
        recommendation: &str,
        comments: Option<&str>,
        evaluation_result: Option<&sqlx::types::JsonValue>,
    ) -> Result<ResearchReview> {
        research::insert_review(
            &self.pool,
            paper_id,
            reviewer_id,
            score,
            recommendation,
            comments,
            evaluation_result,
        )
        .await
    }

    pub async fn elo_award_state(
        &self,
        paper_id: Uuid,
    ) -> Result<Option<(Option<i32>, Option<chrono::DateTime<chrono::Utc>>)>> {
        research::elo_award_state(&self.pool, paper_id).await
    }

    pub async fn elo_awarded(&self, paper_id: Uuid) -> Result<Option<bool>> {
        research::elo_awarded(&self.pool, paper_id).await
    }

    pub async fn complete_review(
        &self,
        paper_id: Uuid,
        reviewer_id: Uuid,
        score: Option<f64>,
        recommendation: &str,
        comments: Option<&str>,
        evaluation_result: Option<&sqlx::types::JsonValue>,
    ) -> Result<Option<ResearchPaper>> {
        research_review::complete_research_review(
            &self.pool,
            paper_id,
            reviewer_id,
            score,
            recommendation,
            comments,
            evaluation_result,
        )
        .await
    }

    pub async fn publish_and_award_elo(
        &self,
        paper_id: Uuid,
        elo_award: i32,
    ) -> Result<Option<ResearchPaper>> {
        research_review::publish_and_award_elo(&self.pool, paper_id, elo_award).await
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
