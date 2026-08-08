use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// The states a research paper can occupy.
///
/// The database stores this value as text so that adding a state does not
/// require replacing a PostgreSQL enum type.  The transition order is still
/// enforced by the research migration and by the write queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchPaperStatus {
    Draft,
    Submitted,
    UnderReview,
    Approved,
    Rejected,
    Published,
}

impl ResearchPaperStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::UnderReview => "under_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Published => "published",
        }
    }
}

pub type PaperStatus = ResearchPaperStatus;
pub type ResearchStatus = ResearchPaperStatus;

impl std::fmt::Display for ResearchPaperStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ResearchPaperStatus {
    type Error = InvalidResearchPaperStatus;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "draft" => Ok(Self::Draft),
            "submitted" => Ok(Self::Submitted),
            "under_review" => Ok(Self::UnderReview),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "published" => Ok(Self::Published),
            _ => Err(InvalidResearchPaperStatus),
        }
    }
}

impl TryFrom<String> for ResearchPaperStatus {
    type Error = InvalidResearchPaperStatus;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidResearchPaperStatus;

impl std::fmt::Display for InvalidResearchPaperStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid research paper status")
    }
}

impl std::error::Error for InvalidResearchPaperStatus {}

/// A persisted research paper and its publication/evaluation state.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ResearchPaper {
    pub id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    pub abstract_text: String,
    pub content: String,
    pub status: String,
    pub submitted_at: Option<DateTime<Utc>>,
    pub under_review_at: Option<DateTime<Utc>>,
    pub decided_by: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub evaluation_score: Option<f64>,
    pub evaluation_result: Option<sqlx::types::JsonValue>,
    pub elo_award: Option<i32>,
    pub elo_awarded: bool,
    pub elo_awarded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResearchPaper {
    pub fn parsed_status(&self) -> Result<ResearchPaperStatus, InvalidResearchPaperStatus> {
        ResearchPaperStatus::try_from(self.status.as_str())
    }
}

/// A review submitted for a paper.  `evaluation_result` preserves structured
/// evaluator output in addition to the normalized score and recommendation.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct ResearchReview {
    pub id: Uuid,
    pub paper_id: Uuid,
    pub reviewer_id: Uuid,
    pub score: Option<f64>,
    pub recommendation: String,
    pub comments: Option<String>,
    pub evaluation_result: Option<sqlx::types::JsonValue>,
    pub reviewed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ResearchReview {
    pub fn parsed_recommendation(
        &self,
    ) -> Result<ReviewRecommendation, InvalidReviewRecommendation> {
        ReviewRecommendation::try_from(self.recommendation.as_str())
    }
}

/// Values accepted by the review transaction for a normalized review result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewRecommendation {
    Approve,
    Reject,
}

impl ReviewRecommendation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

impl std::fmt::Display for ReviewRecommendation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ReviewRecommendation {
    type Error = InvalidReviewRecommendation;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approve" | "approved" => Ok(Self::Approve),
            "reject" | "rejected" => Ok(Self::Reject),
            _ => Err(InvalidReviewRecommendation),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidReviewRecommendation;

impl std::fmt::Display for InvalidReviewRecommendation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid research review recommendation")
    }
}

impl std::error::Error for InvalidReviewRecommendation {}

/// Input for creating a paper in the draft state.
#[derive(Debug, Clone)]
pub struct NewResearchPaper<'a> {
    pub author_id: Uuid,
    pub title: &'a str,
    pub abstract_text: &'a str,
    pub content: &'a str,
}

/// Input for persisting a review.  A review is immutable as a decision from
/// the lifecycle's point of view, but its evaluator payload may be updated by
/// a retry of the same worker job.
#[derive(Debug, Clone)]
pub struct NewResearchReview<'a> {
    pub paper_id: Uuid,
    pub reviewer_id: Uuid,
    pub score: Option<f64>,
    pub recommendation: &'a str,
    pub comments: Option<&'a str>,
    pub evaluation_result: Option<&'a sqlx::types::JsonValue>,
}
