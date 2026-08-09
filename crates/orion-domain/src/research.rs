use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{Role, UserId, VersionedEvent};

/// The authoritative lifecycle states for a research paper.
///
/// A decided paper is immutable from the lifecycle's point of view.  A
/// revision is represented by a new paper row which starts a new cycle at
/// [`ResearchStatus::Draft`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchStatus {
    Draft,
    Submitted,
    UnderReview,
    Approved,
    Rejected,
    Published,
}

/// Persistence-facing aliases used by the database feature.
pub type ResearchPaperStatus = ResearchStatus;
pub type PaperStatus = ResearchStatus;

/// The legal in-place lifecycle transitions for a research paper.
pub const LEGAL_RESEARCH_TRANSITIONS: &[(ResearchStatus, ResearchStatus)] = &[
    (ResearchStatus::Draft, ResearchStatus::Submitted),
    (ResearchStatus::Submitted, ResearchStatus::UnderReview),
    (ResearchStatus::UnderReview, ResearchStatus::Approved),
    (ResearchStatus::UnderReview, ResearchStatus::Rejected),
    (ResearchStatus::Approved, ResearchStatus::Published),
];

/// Domain representation of a research paper.
///
/// Persistence adapters map their database rows into this type so lifecycle
/// changes are made with [`ResearchStatus::transition`] instead of mutating a
/// status string directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchPaper {
    pub id: Uuid,
    pub author_id: UserId,
    pub title: String,
    pub abstract_text: String,
    pub content: String,
    pub status: ResearchStatus,
}

impl ResearchPaper {
    /// Creates a paper at the only legal initial state: `draft`.
    #[must_use]
    pub fn new(
        id: Uuid,
        author_id: UserId,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id,
            author_id,
            title: title.into(),
            abstract_text: abstract_text.into(),
            content: content.into(),
            status: ResearchStatus::Draft,
        }
    }

    /// Fallible constructor for adapters that want validation at the boundary.
    pub fn try_new(
        id: Uuid,
        author_id: UserId,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Self, ResearchContractError> {
        let paper = Self::new(id, author_id, title, abstract_text, content);
        paper.validate()?;
        Ok(paper)
    }

    /// Validates the entity fields that are required by the research tables.
    pub fn validate(&self) -> Result<(), ResearchContractError> {
        validate_paper_fields(self.id, self.author_id, &self.title, &self.content)
    }

    /// Applies one legal in-place transition.
    pub fn transition(&mut self, next: ResearchStatus) -> Result<(), ResearchContractError> {
        self.validate()?;
        let next_status = self.status.transition(next)?;
        if matches!(next, ResearchStatus::Approved | ResearchStatus::Rejected) {
            return Err(ResearchContractError::DecisionRequiresReview);
        }
        self.status = next_status;
        Ok(())
    }

    /// Applies an approval or rejection only when the review, reviewer, and
    /// paper identity have all been validated.
    pub fn transition_with_review(
        &mut self,
        review: &ResearchReview,
        reviewer: &ResearchReviewer,
    ) -> Result<(), ResearchContractError> {
        self.validate()?;
        review.validate_for(self, reviewer)?;
        let next = match review.recommendation() {
            EvaluationRecommendation::Approve => ResearchStatus::Approved,
            EvaluationRecommendation::Reject => ResearchStatus::Rejected,
        };
        self.status = self.status.transition(next)?;
        Ok(())
    }

    /// Draft content may be edited in place.  Once submitted, content is
    /// immutable and must be changed through a new revision identity.
    pub fn edit_content(
        &mut self,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<(), ResearchContractError> {
        if self.status != ResearchStatus::Draft {
            return Err(ResearchContractError::ContentEditNotAllowed {
                status: self.status,
            });
        }
        let title = title.into();
        let abstract_text = abstract_text.into();
        let content = content.into();
        validate_paper_fields(self.id, self.author_id, &title, &content)?;
        self.title = title;
        self.abstract_text = abstract_text;
        self.content = content;
        Ok(())
    }

    /// Creates a new paper entity for a post-decision revision.  The original
    /// entity remains unchanged and the returned entity starts in `draft`.
    pub fn create_revision(&self, id: Uuid) -> Result<Self, ResearchContractError> {
        self.create_revision_with_content(
            id,
            self.title.clone(),
            self.abstract_text.clone(),
            self.content.clone(),
        )
    }

    /// Creates a new draft with replacement content after a decision.  The
    /// caller must persist the returned entity as a new paper ID.
    pub fn create_revision_with_content(
        &self,
        id: Uuid,
        title: impl Into<String>,
        abstract_text: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Self, ResearchContractError> {
        self.status.start_revision().and_then(|status| {
            let revision = Self {
                id,
                author_id: self.author_id,
                title: title.into(),
                abstract_text: abstract_text.into(),
                content: content.into(),
                status,
            };
            revision.validate()?;
            Ok(revision)
        })
    }
}

fn validate_paper_fields(
    id: Uuid,
    author_id: UserId,
    title: &str,
    content: &str,
) -> Result<(), ResearchContractError> {
    if id.is_nil() {
        return Err(ResearchContractError::NilPaperId);
    }
    if author_id.into_uuid().is_nil() {
        return Err(ResearchContractError::NilAuthorId);
    }
    if title.trim().is_empty() {
        return Err(ResearchContractError::EmptyPaperTitle);
    }
    if content.trim().is_empty() {
        return Err(ResearchContractError::EmptyPaperContent);
    }
    Ok(())
}

/// The identity and authorization context required to review research.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReviewer {
    pub user_id: UserId,
    pub role: Role,
}

impl ResearchReviewer {
    #[must_use]
    pub const fn new(user_id: UserId) -> Self {
        Self {
            user_id,
            role: Role::Reviewer,
        }
    }

    /// A reviewer must have the reviewer role, must not be the paper author,
    /// and may only review a paper claimed by the review workflow.
    pub fn can_review(&self, paper: &ResearchPaper) -> bool {
        self.role == Role::Reviewer
            && self.user_id != paper.author_id
            && paper.status == ResearchStatus::UnderReview
    }

    pub fn authorize(&self, paper: &ResearchPaper) -> Result<(), ResearchContractError> {
        paper.validate()?;
        if self.role != Role::Reviewer {
            return Err(ResearchContractError::UnauthorizedReviewerRole { role: self.role });
        }
        if self.user_id == paper.author_id {
            return Err(ResearchContractError::AuthorCannotReview);
        }
        if paper.status != ResearchStatus::UnderReview {
            return Err(ResearchContractError::ReviewNotAllowedFromState {
                status: paper.status,
            });
        }
        Ok(())
    }
}

/// The two legal ways to begin another research review cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchReReviewKind {
    /// A challenge to a rejected decision.
    Appeal,
    /// A content revision after any decided state.
    Revision,
}

/// Request metadata for creating a new paper/review cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReReviewRequest {
    pub source_paper_id: Uuid,
    pub new_paper_id: Uuid,
    pub requester_id: UserId,
    pub kind: ResearchReReviewKind,
    pub reason: String,
}

impl ResearchReReviewRequest {
    /// Creates an appeal for a rejected paper.  The appeal never reopens the
    /// rejected row; it authorizes a new draft identity for re-review.
    pub fn appeal(
        source: &ResearchPaper,
        requester_id: UserId,
        new_paper_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, ResearchContractError> {
        Self::new(
            source,
            requester_id,
            new_paper_id,
            ResearchReReviewKind::Appeal,
            reason,
        )
    }

    /// Creates a content revision request from any decided paper.
    pub fn revision(
        source: &ResearchPaper,
        requester_id: UserId,
        new_paper_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, ResearchContractError> {
        Self::new(
            source,
            requester_id,
            new_paper_id,
            ResearchReReviewKind::Revision,
            reason,
        )
    }

    fn new(
        source: &ResearchPaper,
        requester_id: UserId,
        new_paper_id: Uuid,
        kind: ResearchReReviewKind,
        reason: impl Into<String>,
    ) -> Result<Self, ResearchContractError> {
        let reason = reason.into();
        source.validate()?;
        if requester_id != source.author_id {
            return Err(ResearchContractError::OnlyAuthorMayRequestRevision);
        }
        if source.id == new_paper_id || new_paper_id.is_nil() {
            return Err(ResearchContractError::RevisionMustUseNewPaperId);
        }
        if reason.trim().is_empty() {
            return Err(ResearchContractError::EmptyRevisionReason);
        }
        match (kind, source.status) {
            (ResearchReReviewKind::Appeal, ResearchStatus::Rejected)
            | (ResearchReReviewKind::Revision, ResearchStatus::Approved)
            | (ResearchReReviewKind::Revision, ResearchStatus::Rejected)
            | (ResearchReReviewKind::Revision, ResearchStatus::Published) => Ok(Self {
                source_paper_id: source.id,
                new_paper_id,
                requester_id,
                kind,
                reason,
            }),
            (ResearchReReviewKind::Appeal, _) => {
                Err(ResearchContractError::AppealRequiresRejectedPaper)
            }
            (ResearchReReviewKind::Revision, status) => {
                Err(ResearchContractError::RevisionNotAllowed { status })
            }
        }
    }
}

/// Domain representation of a completed review for a research paper.
///
/// The structured evaluation contains the normalized score and recommendation
/// used by the lifecycle decision.  `comments` is retained as reviewer-facing
/// context and is intentionally separate from the rubric rationale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchReview {
    pub id: Uuid,
    pub paper_id: Uuid,
    pub reviewer_id: UserId,
    pub evaluation: ResearchEvaluationV1,
    pub comments: Option<String>,
}

impl ResearchReview {
    #[must_use]
    pub fn score(&self) -> u8 {
        self.evaluation.overall_score
    }

    #[must_use]
    pub fn recommendation(&self) -> EvaluationRecommendation {
        self.evaluation.recommendation
    }

    pub fn validate_for(
        &self,
        paper: &ResearchPaper,
        reviewer: &ResearchReviewer,
    ) -> Result<(), ResearchContractError> {
        reviewer.authorize(paper)?;
        if self.paper_id != paper.id {
            return Err(ResearchContractError::ReviewPaperIdentityMismatch);
        }
        if self.reviewer_id != reviewer.user_id {
            return Err(ResearchContractError::ReviewerIdentityMismatch);
        }
        if self
            .comments
            .as_deref()
            .is_some_and(|comments| comments.trim().is_empty())
        {
            return Err(ResearchContractError::EmptyReviewComments);
        }
        self.evaluation.validate()
    }
}

impl ResearchStatus {
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

    /// Returns whether the requested state is one of the approved lifecycle
    /// transitions.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Submitted)
                | (Self::Submitted, Self::UnderReview)
                | (Self::UnderReview, Self::Approved)
                | (Self::UnderReview, Self::Rejected)
                | (Self::Approved, Self::Published)
        )
    }

    /// Applies one lifecycle transition without mutating persistence.
    pub fn transition(self, next: Self) -> Result<Self, ResearchContractError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(ResearchContractError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }

    /// A decision exists after approval or rejection.  Publication is also a
    /// decided state, but it cannot be edited in place.
    pub const fn is_decided(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected | Self::Published)
    }

    /// Returns whether a new paper may be created from this state.
    pub const fn can_create_revision(self) -> bool {
        self.is_decided()
    }

    /// A revision always starts a new, independent review cycle.
    pub fn start_revision(self) -> Result<Self, ResearchContractError> {
        if self.can_create_revision() {
            Ok(Self::Draft)
        } else {
            Err(ResearchContractError::RevisionNotAllowed { status: self })
        }
    }
}

impl fmt::Display for ResearchStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for ResearchStatus {
    type Error = ResearchContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "draft" => Ok(Self::Draft),
            "submitted" => Ok(Self::Submitted),
            "under_review" => Ok(Self::UnderReview),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "published" => Ok(Self::Published),
            _ => Err(ResearchContractError::InvalidStatus(value.to_owned())),
        }
    }
}

impl TryFrom<String> for ResearchStatus {
    type Error = ResearchContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

/// Errors raised while applying the domain contract before persistence.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResearchContractError {
    #[error("invalid research status: {0}")]
    InvalidStatus(String),
    #[error("invalid research review recommendation: {0}")]
    InvalidRecommendation(String),
    #[error("invalid research lifecycle transition: {from} -> {to}")]
    InvalidTransition {
        from: ResearchStatus,
        to: ResearchStatus,
    },
    #[error("research revision is not allowed from state {status}")]
    RevisionNotAllowed { status: ResearchStatus },
    #[error("role {role:?} is not authorized to review research")]
    UnauthorizedReviewerRole { role: Role },
    #[error("a paper author cannot review their own paper")]
    AuthorCannotReview,
    #[error("research review is not allowed from state {status}")]
    ReviewNotAllowedFromState { status: ResearchStatus },
    #[error("the review identity does not match the supplied reviewer")]
    ReviewerIdentityMismatch,
    #[error("the review does not belong to the supplied paper")]
    ReviewPaperIdentityMismatch,
    #[error("paper approval or rejection requires a validated review")]
    DecisionRequiresReview,
    #[error("research paper ID must not be nil")]
    NilPaperId,
    #[error("research paper author ID must not be nil")]
    NilAuthorId,
    #[error("research paper title must not be empty")]
    EmptyPaperTitle,
    #[error("research paper content must not be empty")]
    EmptyPaperContent,
    #[error("research review comments must not be empty when supplied")]
    EmptyReviewComments,
    #[error("research content cannot be edited from state {status}")]
    ContentEditNotAllowed { status: ResearchStatus },
    #[error("only the paper author may request an appeal or revision")]
    OnlyAuthorMayRequestRevision,
    #[error("an appeal requires a new paper ID")]
    RevisionMustUseNewPaperId,
    #[error("a revision or appeal requires a non-empty reason")]
    EmptyRevisionReason,
    #[error("an appeal requires a rejected paper")]
    AppealRequiresRejectedPaper,
    #[error("a research Elo award requires a published paper")]
    EloAwardRequiresPublishedPaper,
    #[error("a research Elo award requires an approved evaluation")]
    EloAwardRequiresApproval,
    #[error("invalid research Elo award idempotency key")]
    InvalidEloAwardIdempotencyKey,
    #[error("unsupported research evaluation rubric version: {0}")]
    UnsupportedRubricVersion(u16),
    #[error("research evaluation content version must be greater than zero")]
    InvalidEvaluatedContentVersion,
    #[error("research rubric score for {criterion} must be between 0 and 100, got {score}")]
    InvalidScore { criterion: &'static str, score: u8 },
    #[error("research evaluation overall score must be between 0 and 100, got {0}")]
    InvalidOverallScore(u8),
    #[error("research evaluation overall score {actual} does not match weighted score {expected}")]
    OverallScoreMismatch { expected: u8, actual: u8 },
    #[error("research evaluation rationale must not be empty")]
    EmptyRationale,
    #[error("research evaluation requires at least one evidence item")]
    MissingEvidence,
    #[error("research evidence {field} must not be empty")]
    EmptyEvidenceField { field: &'static str },
    #[error("research evaluation requires at least one {field} item")]
    MissingFeedback { field: &'static str },
    #[error("research evaluation {field} must not contain an empty item")]
    EmptyFeedbackItem { field: &'static str },
}

/// Canonical review decisions.  The parser accepts the legacy plural forms
/// already present in `research_reviews`, while serialization emits only the
/// canonical values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationRecommendation {
    #[serde(rename = "approve", alias = "approved")]
    Approve,
    #[serde(rename = "reject", alias = "rejected")]
    Reject,
}

pub type ResearchRecommendation = EvaluationRecommendation;
pub type ReviewRecommendation = EvaluationRecommendation;

impl EvaluationRecommendation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

impl fmt::Display for EvaluationRecommendation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<&str> for EvaluationRecommendation {
    type Error = ResearchContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approve" | "approved" => Ok(Self::Approve),
            "reject" | "rejected" => Ok(Self::Reject),
            _ => Err(ResearchContractError::InvalidRecommendation(
                value.to_owned(),
            )),
        }
    }
}

impl TryFrom<String> for EvaluationRecommendation {
    type Error = ResearchContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

/// Version of the structured evaluation payload stored in
/// `research_reviews.evaluation_result` and
/// `research_papers.evaluation_result`.
pub const RESEARCH_EVALUATION_RUBRIC_VERSION: u16 = 1;

/// Scores are normalized to a 0-100 scale for every rubric dimension.
pub const MIN_RUBRIC_SCORE: u8 = 0;
pub const MAX_RUBRIC_SCORE: u8 = 100;

/// A concrete, reviewable piece of evidence supporting an evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEvidence {
    /// A paper section, dataset, artifact, citation, or other traceable source.
    pub reference: String,
    /// The finding or observation that the reviewer is relying on.
    pub finding: String,
}

impl ResearchEvidence {
    pub fn validate(&self) -> Result<(), ResearchContractError> {
        if self.reference.trim().is_empty() {
            return Err(ResearchContractError::EmptyEvidenceField { field: "reference" });
        }
        if self.finding.trim().is_empty() {
            return Err(ResearchContractError::EmptyEvidenceField { field: "finding" });
        }
        Ok(())
    }
}

/// A complete research evaluation.  The scalar `score` and
/// `recommendation` columns remain query-friendly projections; this value is
/// the authoritative shape of the structured evaluation JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEvaluationV1 {
    pub rubric_version: u16,
    pub evaluated_content_version: u32,
    pub scores: ResearchRubricScores,
    pub overall_score: u8,
    pub recommendation: EvaluationRecommendation,
    pub rationale: String,
    pub evidence: Vec<ResearchEvidence>,
    pub strengths: Vec<String>,
    pub concerns: Vec<String>,
}

pub type ResearchEvaluation = ResearchEvaluationV1;

impl ResearchEvaluationV1 {
    pub const RUBRIC_VERSION: u16 = RESEARCH_EVALUATION_RUBRIC_VERSION;

    /// Builds a complete evaluation and derives the overall score from the
    /// rubric dimensions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rubric_version: u16,
        evaluated_content_version: u32,
        scores: ResearchRubricScores,
        recommendation: EvaluationRecommendation,
        rationale: impl Into<String>,
        evidence: Vec<ResearchEvidence>,
        strengths: Vec<String>,
        concerns: Vec<String>,
    ) -> Result<Self, ResearchContractError> {
        let evaluation = Self {
            rubric_version,
            evaluated_content_version,
            overall_score: scores.weighted_score(),
            scores,
            recommendation,
            rationale: rationale.into(),
            evidence,
            strengths,
            concerns,
        };
        evaluation.validate()?;
        Ok(evaluation)
    }

    /// Validates a payload read from either completed research table.
    pub fn validate(&self) -> Result<(), ResearchContractError> {
        if self.rubric_version != Self::RUBRIC_VERSION {
            return Err(ResearchContractError::UnsupportedRubricVersion(
                self.rubric_version,
            ));
        }

        if self.evaluated_content_version == 0 {
            return Err(ResearchContractError::InvalidEvaluatedContentVersion);
        }

        self.scores.validate()?;

        if self.overall_score > MAX_RUBRIC_SCORE {
            return Err(ResearchContractError::InvalidOverallScore(
                self.overall_score,
            ));
        }

        let expected = self.scores.weighted_score();
        if self.overall_score != expected {
            return Err(ResearchContractError::OverallScoreMismatch {
                expected,
                actual: self.overall_score,
            });
        }

        if self.rationale.trim().is_empty() {
            return Err(ResearchContractError::EmptyRationale);
        }
        if self.evidence.is_empty() {
            return Err(ResearchContractError::MissingEvidence);
        }
        for evidence in &self.evidence {
            evidence.validate()?;
        }
        validate_feedback_items("strengths", &self.strengths)?;
        validate_feedback_items("concerns", &self.concerns)?;
        Ok(())
    }

    /// Approval is an explicit reviewer recommendation.  The score is
    /// available for ranking and policy checks, but is not silently converted
    /// into a decision by this contract.
    pub const fn recommends_approval(&self) -> bool {
        matches!(self.recommendation, EvaluationRecommendation::Approve)
    }
}

/// Versioned request from Phantom's research evaluator to Yash's Elo consumer.
///
/// Phantom owns the rubric and emits the validated evaluation facts. Yash owns
/// the score-to-Elo policy and must calculate and apply any award exactly once
/// using `idempotency_key`; this contract deliberately carries no Elo delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchEloAwardRequestV1 {
    pub paper_id: Uuid,
    pub author_id: UserId,
    pub paper_status: ResearchStatus,
    pub rubric_version: u16,
    pub evaluated_content_version: u32,
    pub evaluation_score: u8,
    pub recommendation: EvaluationRecommendation,
    pub idempotency_key: String,
}

impl ResearchEloAwardRequestV1 {
    pub const EVENT_TYPE: &'static str = "orion.research.elo_award.requested";
    pub const SCHEMA_VERSION: u16 = 1;

    /// Creates a request only after publication and an approved evaluation.
    pub fn for_published_paper(
        paper: &ResearchPaper,
        evaluation: &ResearchEvaluationV1,
    ) -> Result<Self, ResearchContractError> {
        paper.validate()?;
        evaluation.validate()?;
        let request = Self {
            paper_id: paper.id,
            author_id: paper.author_id,
            paper_status: paper.status,
            rubric_version: evaluation.rubric_version,
            evaluated_content_version: evaluation.evaluated_content_version,
            evaluation_score: evaluation.overall_score,
            recommendation: evaluation.recommendation,
            idempotency_key: Self::idempotency_key(paper.id),
        };
        request.validate()?;
        Ok(request)
    }

    #[must_use]
    pub fn idempotency_key(paper_id: Uuid) -> String {
        format!("research-paper:{paper_id}:elo-award")
    }

    pub fn validate(&self) -> Result<(), ResearchContractError> {
        if self.paper_status != ResearchStatus::Published {
            return Err(ResearchContractError::EloAwardRequiresPublishedPaper);
        }
        if self.recommendation != EvaluationRecommendation::Approve {
            return Err(ResearchContractError::EloAwardRequiresApproval);
        }
        if self.evaluation_score > MAX_RUBRIC_SCORE {
            return Err(ResearchContractError::InvalidOverallScore(
                self.evaluation_score,
            ));
        }
        if self.rubric_version != RESEARCH_EVALUATION_RUBRIC_VERSION {
            return Err(ResearchContractError::UnsupportedRubricVersion(
                self.rubric_version,
            ));
        }
        if self.evaluated_content_version == 0 {
            return Err(ResearchContractError::InvalidEvaluatedContentVersion);
        }
        if self.idempotency_key != Self::idempotency_key(self.paper_id) {
            return Err(ResearchContractError::InvalidEloAwardIdempotencyKey);
        }
        Ok(())
    }
}

impl VersionedEvent for ResearchEloAwardRequestV1 {
    const EVENT_TYPE: &'static str = ResearchEloAwardRequestV1::EVENT_TYPE;
    const SCHEMA_VERSION: u16 = ResearchEloAwardRequestV1::SCHEMA_VERSION;
}

fn validate_feedback_items(
    field: &'static str,
    items: &[String],
) -> Result<(), ResearchContractError> {
    if items.is_empty() {
        return Err(ResearchContractError::MissingFeedback { field });
    }
    if items.iter().any(|item| item.trim().is_empty()) {
        return Err(ResearchContractError::EmptyFeedbackItem { field });
    }
    Ok(())
}

/// The five required dimensions and their weights in rubric version 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchRubricScores {
    /// Relevance and precision of the research question (15%).
    pub relevance: u8,
    /// Soundness and appropriateness of the method (25%).
    pub methodology: u8,
    /// Quality and sufficiency of evidence or results (30%).
    pub evidence: u8,
    /// Originality and usefulness of the contribution (15%).
    pub originality: u8,
    /// Clarity and reproducibility of the submission (15%).
    pub clarity_and_reproducibility: u8,
}

impl ResearchRubricScores {
    pub const fn weighted_score(self) -> u8 {
        ((self.relevance as u32 * 15
            + self.methodology as u32 * 25
            + self.evidence as u32 * 30
            + self.originality as u32 * 15
            + self.clarity_and_reproducibility as u32 * 15)
            / 100) as u8
    }

    pub fn validate(self) -> Result<(), ResearchContractError> {
        for (criterion, score) in [
            ("relevance", self.relevance),
            ("methodology", self.methodology),
            ("evidence", self.evidence),
            ("originality", self.originality),
            (
                "clarity_and_reproducibility",
                self.clarity_and_reproducibility,
            ),
        ] {
            if !(MIN_RUBRIC_SCORE..=MAX_RUBRIC_SCORE).contains(&score) {
                return Err(ResearchContractError::InvalidScore { criterion, score });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCORES: ResearchRubricScores = ResearchRubricScores {
        relevance: 80,
        methodology: 90,
        evidence: 85,
        originality: 75,
        clarity_and_reproducibility: 95,
    };

    fn valid_review(
        paper_id: Uuid,
        reviewer_id: UserId,
        recommendation: EvaluationRecommendation,
    ) -> ResearchReview {
        ResearchReview {
            id: Uuid::from_u128(900),
            paper_id,
            reviewer_id,
            evaluation: ResearchEvaluationV1::new(
                RESEARCH_EVALUATION_RUBRIC_VERSION,
                1,
                SCORES,
                recommendation,
                "The evaluation is supported by the cited evidence.",
                vec![ResearchEvidence {
                    reference: "Results section".to_owned(),
                    finding: "The reported result is traceable.".to_owned(),
                }],
                vec!["The method is clear".to_owned()],
                vec!["The sample could be larger".to_owned()],
            )
            .expect("valid review evaluation"),
            comments: Some("Review completed.".to_owned()),
        }
    }

    #[test]
    fn lifecycle_accepts_only_the_approved_forward_transitions() {
        let transitions = [
            (ResearchStatus::Draft, ResearchStatus::Submitted),
            (ResearchStatus::Submitted, ResearchStatus::UnderReview),
            (ResearchStatus::UnderReview, ResearchStatus::Approved),
            (ResearchStatus::UnderReview, ResearchStatus::Rejected),
            (ResearchStatus::Approved, ResearchStatus::Published),
        ];

        for (from, to) in transitions {
            assert_eq!(from.transition(to), Ok(to));
        }

        for (from, to) in [
            (ResearchStatus::Draft, ResearchStatus::UnderReview),
            (ResearchStatus::Submitted, ResearchStatus::Approved),
            (ResearchStatus::Rejected, ResearchStatus::Published),
            (ResearchStatus::Published, ResearchStatus::Draft),
        ] {
            assert!(matches!(
                from.transition(to),
                Err(ResearchContractError::InvalidTransition { .. })
            ));
        }
    }

    #[test]
    fn state_machine_accepts_exactly_the_declared_edges() {
        let states = [
            ResearchStatus::Draft,
            ResearchStatus::Submitted,
            ResearchStatus::UnderReview,
            ResearchStatus::Approved,
            ResearchStatus::Rejected,
            ResearchStatus::Published,
        ];

        for from in states {
            for to in states {
                let declared = LEGAL_RESEARCH_TRANSITIONS.contains(&(from, to));
                assert_eq!(
                    from.can_transition_to(to),
                    declared,
                    "unexpected transition {from} -> {to}"
                );
                assert_eq!(from.transition(to).is_ok(), declared);
            }
        }
    }

    #[test]
    fn paper_transition_guards_cover_every_allowed_and_forbidden_edge() {
        let states = [
            ResearchStatus::Draft,
            ResearchStatus::Submitted,
            ResearchStatus::UnderReview,
            ResearchStatus::Approved,
            ResearchStatus::Rejected,
            ResearchStatus::Published,
        ];

        for from in states {
            for to in states {
                let mut paper = ResearchPaper::new(
                    Uuid::from_u128(100),
                    UserId::from_uuid(Uuid::from_u128(101)),
                    "Title",
                    "Abstract",
                    "Content",
                );
                paper.status = from;

                match (from, to) {
                    (ResearchStatus::Draft, ResearchStatus::Submitted)
                    | (ResearchStatus::Submitted, ResearchStatus::UnderReview)
                    | (ResearchStatus::Approved, ResearchStatus::Published) => {
                        assert_eq!(paper.transition(to), Ok(()));
                    }
                    (ResearchStatus::UnderReview, ResearchStatus::Approved)
                    | (ResearchStatus::UnderReview, ResearchStatus::Rejected) => {
                        assert!(matches!(
                            paper.transition(to),
                            Err(ResearchContractError::DecisionRequiresReview)
                        ));
                    }
                    _ => assert!(matches!(
                        paper.transition(to),
                        Err(ResearchContractError::InvalidTransition {
                            from: actual_from,
                            to: actual_to,
                        }) if actual_from == from && actual_to == to
                    )),
                }
            }
        }
    }

    #[test]
    fn decisions_start_a_new_revision_cycle_at_draft() {
        for status in [
            ResearchStatus::Approved,
            ResearchStatus::Rejected,
            ResearchStatus::Published,
        ] {
            assert_eq!(status.start_revision(), Ok(ResearchStatus::Draft));
        }
        assert!(ResearchStatus::UnderReview.start_revision().is_err());
    }

    #[test]
    fn paper_entity_enforces_transitions_and_revision_immutability() {
        let author_id = UserId::from_uuid(Uuid::from_u128(1));
        let mut paper = ResearchPaper::new(
            Uuid::from_u128(2),
            author_id,
            "Title",
            "Abstract",
            "Content",
        );
        assert_eq!(paper.status, ResearchStatus::Draft);

        paper
            .transition(ResearchStatus::Submitted)
            .expect("draft should submit");
        paper
            .transition(ResearchStatus::UnderReview)
            .expect("submitted paper should enter review");
        assert!(matches!(
            paper.transition(ResearchStatus::Approved),
            Err(ResearchContractError::DecisionRequiresReview)
        ));
        let reviewer_id = UserId::from_uuid(Uuid::from_u128(9));
        let review = valid_review(paper.id, reviewer_id, EvaluationRecommendation::Reject);
        paper
            .transition_with_review(&review, &ResearchReviewer::new(reviewer_id))
            .expect("under-review paper should be rejectable with a review");

        let revision = paper
            .create_revision(Uuid::from_u128(3))
            .expect("decided paper should create a revision");
        assert_eq!(paper.status, ResearchStatus::Rejected);
        assert_eq!(revision.status, ResearchStatus::Draft);
        assert_eq!(revision.id, Uuid::from_u128(3));
        assert_eq!(revision.content, paper.content);
    }

    #[test]
    fn appeal_and_revision_rules_start_new_review_cycles() {
        let author_id = UserId::from_uuid(Uuid::from_u128(4));
        let reviewer_id = UserId::from_uuid(Uuid::from_u128(5));
        let mut paper = ResearchPaper::new(
            Uuid::from_u128(6),
            author_id,
            "Title",
            "Abstract",
            "Content",
        );
        paper
            .edit_content("Edited title", "Edited abstract", "Edited content")
            .expect("draft content is editable");
        for status in [ResearchStatus::Submitted, ResearchStatus::UnderReview] {
            paper.transition(status).expect("legal paper transition");
        }
        let review = valid_review(paper.id, reviewer_id, EvaluationRecommendation::Reject);
        paper
            .transition_with_review(&review, &ResearchReviewer::new(reviewer_id))
            .expect("under-review paper should be rejectable with a review");

        assert!(matches!(
            paper.edit_content("No", "No", "No"),
            Err(ResearchContractError::ContentEditNotAllowed {
                status: ResearchStatus::Rejected
            })
        ));

        for (status, revision_id) in [
            (ResearchStatus::Approved, Uuid::from_u128(70)),
            (ResearchStatus::Published, Uuid::from_u128(71)),
        ] {
            let mut decided = ResearchPaper::new(
                Uuid::from_u128(72),
                author_id,
                "Decided title",
                "Decided abstract",
                "Decided content",
            );
            decided.status = status;
            assert!(matches!(
                decided.edit_content("Changed", "Changed", "Changed"),
                Err(ResearchContractError::ContentEditNotAllowed { status: actual })
                    if actual == status
            ));
            assert_eq!(
                decided
                    .create_revision(revision_id)
                    .expect("decided content changes use a new revision")
                    .status,
                ResearchStatus::Draft
            );
        }

        let appeal = ResearchReReviewRequest::appeal(
            &paper,
            author_id,
            Uuid::from_u128(7),
            "Additional evidence addresses the rejection.",
        )
        .expect("author may appeal a rejection");
        assert_eq!(appeal.kind, ResearchReReviewKind::Appeal);

        let revised = paper
            .create_revision_with_content(
                appeal.new_paper_id,
                "Revised title",
                "Revised abstract",
                "Revised content",
            )
            .expect("appeal creates a new draft");
        assert_eq!(paper.status, ResearchStatus::Rejected);
        assert_eq!(revised.status, ResearchStatus::Draft);
        assert_eq!(revised.content, "Revised content");

        assert!(matches!(
            ResearchReReviewRequest::appeal(
                &paper,
                reviewer_id,
                Uuid::from_u128(8),
                "not the author"
            ),
            Err(ResearchContractError::OnlyAuthorMayRequestRevision)
        ));
        assert!(matches!(
            ResearchReReviewRequest::appeal(&paper, author_id, paper.id, "same row"),
            Err(ResearchContractError::RevisionMustUseNewPaperId)
        ));
        assert!(matches!(
            ResearchReReviewRequest::appeal(&paper, author_id, Uuid::from_u128(8), "  "),
            Err(ResearchContractError::EmptyRevisionReason)
        ));
    }

    #[test]
    fn evaluation_derives_and_validates_the_weighted_score() {
        let evaluation = ResearchEvaluationV1::new(
            RESEARCH_EVALUATION_RUBRIC_VERSION,
            1,
            SCORES,
            EvaluationRecommendation::Approve,
            "The result is supported by a reproducible method.",
            vec![ResearchEvidence {
                reference: "Results section".to_owned(),
                finding: "The reported result matches the described method.".to_owned(),
            }],
            vec!["Clear evidence".to_owned()],
            vec!["Small sample".to_owned()],
        )
        .expect("valid evaluation");

        assert_eq!(evaluation.overall_score, 85);
        assert!(evaluation.recommends_approval());
        evaluation.validate().expect("evaluation remains valid");
    }

    #[test]
    fn rubric_score_boundaries_and_weights_are_deterministic() {
        let zero = ResearchRubricScores {
            relevance: 0,
            methodology: 0,
            evidence: 0,
            originality: 0,
            clarity_and_reproducibility: 0,
        };
        assert_eq!(zero.weighted_score(), 0);
        assert!(zero.validate().is_ok());

        let maximum = ResearchRubricScores {
            relevance: 100,
            methodology: 100,
            evidence: 100,
            originality: 100,
            clarity_and_reproducibility: 100,
        };
        assert_eq!(maximum.weighted_score(), 100);
        assert!(maximum.validate().is_ok());

        for (criterion, expected) in [
            ("relevance", 15),
            ("methodology", 25),
            ("evidence", 30),
            ("originality", 15),
            ("clarity_and_reproducibility", 15),
        ] {
            let scores = match criterion {
                "relevance" => ResearchRubricScores {
                    relevance: 100,
                    ..zero
                },
                "methodology" => ResearchRubricScores {
                    methodology: 100,
                    ..zero
                },
                "evidence" => ResearchRubricScores {
                    evidence: 100,
                    ..zero
                },
                "originality" => ResearchRubricScores {
                    originality: 100,
                    ..zero
                },
                "clarity_and_reproducibility" => ResearchRubricScores {
                    clarity_and_reproducibility: 100,
                    ..zero
                },
                _ => unreachable!(),
            };
            assert_eq!(scores.weighted_score(), expected);
        }
    }

    #[test]
    fn rubric_rejects_invalid_versions_scores_and_payload_shape() {
        let mut unsupported = valid_review(
            Uuid::from_u128(700),
            UserId::from_uuid(Uuid::from_u128(701)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        unsupported.rubric_version = 2;
        assert!(matches!(
            unsupported.validate(),
            Err(ResearchContractError::UnsupportedRubricVersion(2))
        ));

        let mut missing_content_version = valid_review(
            Uuid::from_u128(701),
            UserId::from_uuid(Uuid::from_u128(702)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        missing_content_version.evaluated_content_version = 0;
        assert!(matches!(
            missing_content_version.validate(),
            Err(ResearchContractError::InvalidEvaluatedContentVersion)
        ));

        let invalid_score = ResearchRubricScores {
            relevance: 101,
            methodology: 0,
            evidence: 0,
            originality: 0,
            clarity_and_reproducibility: 0,
        };
        assert!(matches!(
            invalid_score.validate(),
            Err(ResearchContractError::InvalidScore {
                criterion: "relevance",
                score: 101
            })
        ));

        let mut mismatch = valid_review(
            Uuid::from_u128(702),
            UserId::from_uuid(Uuid::from_u128(703)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        mismatch.overall_score = mismatch.overall_score.saturating_add(1);
        assert!(matches!(
            mismatch.validate(),
            Err(ResearchContractError::OverallScoreMismatch { .. })
        ));

        let mut empty_evidence = valid_review(
            Uuid::from_u128(704),
            UserId::from_uuid(Uuid::from_u128(705)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        empty_evidence.evidence = vec![ResearchEvidence {
            reference: "  ".to_owned(),
            finding: "finding".to_owned(),
        }];
        assert!(matches!(
            empty_evidence.validate(),
            Err(ResearchContractError::EmptyEvidenceField { field: "reference" })
        ));

        let mut empty_feedback = valid_review(
            Uuid::from_u128(706),
            UserId::from_uuid(Uuid::from_u128(707)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        empty_feedback.concerns.clear();
        assert!(matches!(
            empty_feedback.validate(),
            Err(ResearchContractError::MissingFeedback { field: "concerns" })
        ));
    }

    #[test]
    fn rubric_serialization_emits_canonical_contract_values() {
        let evaluation = valid_review(
            Uuid::from_u128(708),
            UserId::from_uuid(Uuid::from_u128(709)),
            EvaluationRecommendation::Approve,
        )
        .evaluation;
        let serialized = serde_json::to_value(&evaluation).expect("serialize evaluation");

        assert_eq!(serialized["rubric_version"], 1);
        assert_eq!(serialized["evaluated_content_version"], 1);
        assert_eq!(serialized["recommendation"], "approve");
        assert_eq!(serialized["evidence"][0]["reference"], "Results section");
        assert!(serde_json::from_value::<ResearchEvaluationV1>(serialized.clone()).is_ok());

        let mut missing_rubric_version = serialized.clone();
        missing_rubric_version
            .as_object_mut()
            .expect("evaluation is an object")
            .remove("rubric_version");
        assert!(serde_json::from_value::<ResearchEvaluationV1>(missing_rubric_version).is_err());

        let mut missing_content_version = serialized;
        missing_content_version
            .as_object_mut()
            .expect("evaluation is an object")
            .remove("evaluated_content_version");
        assert!(serde_json::from_value::<ResearchEvaluationV1>(missing_content_version).is_err());
    }

    #[test]
    fn reviewer_authorization_requires_role_and_review_state() {
        let author_id = UserId::from_uuid(Uuid::from_u128(10));
        let reviewer_id = UserId::from_uuid(Uuid::from_u128(11));
        let mut paper = ResearchPaper::new(
            Uuid::from_u128(12),
            author_id,
            "Title",
            "Abstract",
            "Content",
        );
        let reviewer = ResearchReviewer::new(reviewer_id);

        assert!(!reviewer.can_review(&paper));
        paper
            .transition(ResearchStatus::Submitted)
            .expect("draft should submit");
        paper
            .transition(ResearchStatus::UnderReview)
            .expect("submitted paper should enter review");
        assert!(reviewer.can_review(&paper));
        let author_reviewer = ResearchReviewer::new(author_id);
        assert!(!author_reviewer.can_review(&paper));
        assert!(matches!(
            author_reviewer.authorize(&paper),
            Err(ResearchContractError::AuthorCannotReview)
        ));
        assert!(!ResearchReviewer {
            user_id: reviewer_id,
            role: Role::Admin,
        }
        .can_review(&paper));
    }

    #[test]
    fn evaluation_requires_evidence_and_both_feedback_sections() {
        let missing_evidence = ResearchEvaluationV1::new(
            RESEARCH_EVALUATION_RUBRIC_VERSION,
            1,
            SCORES,
            EvaluationRecommendation::Reject,
            "The method needs more support.",
            Vec::new(),
            vec!["The question is clear".to_owned()],
            vec!["The sample is too small".to_owned()],
        );
        assert!(matches!(
            missing_evidence,
            Err(ResearchContractError::MissingEvidence)
        ));

        let missing_strengths = ResearchEvaluationV1::new(
            RESEARCH_EVALUATION_RUBRIC_VERSION,
            1,
            SCORES,
            EvaluationRecommendation::Reject,
            "The method needs more support.",
            vec![ResearchEvidence {
                reference: "Methods section".to_owned(),
                finding: "The sample size is not justified.".to_owned(),
            }],
            Vec::new(),
            vec!["The sample is too small".to_owned()],
        );
        assert!(matches!(
            missing_strengths,
            Err(ResearchContractError::MissingFeedback { field: "strengths" })
        ));
    }

    #[test]
    fn elo_award_request_requires_published_approved_and_is_idempotent() {
        let author_id = UserId::from_uuid(Uuid::from_u128(30));
        let mut paper = ResearchPaper::new(
            Uuid::from_u128(31),
            author_id,
            "Title",
            "Abstract",
            "Content",
        );
        for status in [ResearchStatus::Submitted, ResearchStatus::UnderReview] {
            paper.transition(status).expect("legal paper transition");
        }
        let reviewer_id = UserId::from_uuid(Uuid::from_u128(32));
        let review = valid_review(paper.id, reviewer_id, EvaluationRecommendation::Approve);
        paper
            .transition_with_review(&review, &ResearchReviewer::new(reviewer_id))
            .expect("under-review paper should be approvable with a review");
        paper
            .transition(ResearchStatus::Published)
            .expect("approved paper should publish");
        let evaluation = ResearchEvaluationV1::new(
            RESEARCH_EVALUATION_RUBRIC_VERSION,
            1,
            SCORES,
            EvaluationRecommendation::Approve,
            "The published result is well-supported.",
            vec![ResearchEvidence {
                reference: "Results section".to_owned(),
                finding: "The reported result is reproducible.".to_owned(),
            }],
            vec!["Clear method".to_owned()],
            vec!["Limited sample".to_owned()],
        )
        .expect("valid evaluation");

        let request = ResearchEloAwardRequestV1::for_published_paper(&paper, &evaluation)
            .expect("published approved paper can request Elo");
        assert_eq!(request.paper_id, paper.id);
        assert_eq!(request.author_id, author_id);
        assert_eq!(request.evaluation_score, evaluation.overall_score);
        assert_eq!(
            request.evaluated_content_version,
            evaluation.evaluated_content_version
        );
        assert_eq!(
            request.idempotency_key,
            ResearchEloAwardRequestV1::idempotency_key(paper.id)
        );
        assert_eq!(
            <ResearchEloAwardRequestV1 as VersionedEvent>::EVENT_TYPE,
            "orion.research.elo_award.requested"
        );
        let serialized = serde_json::to_value(&request).expect("serialize Elo request");
        assert!(serialized.get("elo_award").is_none());
        request.validate().expect("request remains valid");
    }

    #[test]
    fn elo_award_request_rejects_unpublished_papers_and_tampering() {
        let author_id = UserId::from_uuid(Uuid::from_u128(40));
        let paper = ResearchPaper::new(
            Uuid::from_u128(41),
            author_id,
            "Title",
            "Abstract",
            "Content",
        );
        let evaluation = ResearchEvaluationV1::new(
            RESEARCH_EVALUATION_RUBRIC_VERSION,
            1,
            SCORES,
            EvaluationRecommendation::Approve,
            "The evidence is sufficient.",
            vec![ResearchEvidence {
                reference: "Appendix A".to_owned(),
                finding: "The calculation is shown.".to_owned(),
            }],
            vec!["Transparent calculation".to_owned()],
            vec!["No material concern".to_owned()],
        )
        .expect("valid evaluation");

        assert!(matches!(
            ResearchEloAwardRequestV1::for_published_paper(&paper, &evaluation),
            Err(ResearchContractError::EloAwardRequiresPublishedPaper)
        ));

        let mut published = paper;
        for status in [ResearchStatus::Submitted, ResearchStatus::UnderReview] {
            published
                .transition(status)
                .expect("legal paper transition");
        }
        let reviewer_id = UserId::from_uuid(Uuid::from_u128(42));
        let review = valid_review(published.id, reviewer_id, EvaluationRecommendation::Approve);
        published
            .transition_with_review(&review, &ResearchReviewer::new(reviewer_id))
            .expect("under-review paper should be approvable with a review");
        published
            .transition(ResearchStatus::Published)
            .expect("approved paper should publish");
        let mut request = ResearchEloAwardRequestV1::for_published_paper(&published, &evaluation)
            .expect("published approved paper can request Elo");
        request.idempotency_key.push_str(":tampered");
        assert!(matches!(
            request.validate(),
            Err(ResearchContractError::InvalidEloAwardIdempotencyKey)
        ));
    }

    #[test]
    fn evaluation_recommendation_accepts_completed_table_spellings() {
        assert_eq!(
            EvaluationRecommendation::try_from("approved").expect("approved spelling"),
            EvaluationRecommendation::Approve
        );
        assert_eq!(
            EvaluationRecommendation::try_from("rejected").expect("rejected spelling"),
            EvaluationRecommendation::Reject
        );
    }
}
