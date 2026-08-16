/** Contract-derived from `orion-api::routes::research::ResearchPaperResponse`. */
export const RESEARCH_STATUSES = [
  "draft",
  "submitted",
  "under_review",
  "approved",
  "rejected",
  "published",
] as const;

export type ResearchStatus = (typeof RESEARCH_STATUSES)[number];

/** The lifecycle can show the completed award stage after publication. */
export type ResearchLifecycleStatus = ResearchStatus | "awarded";

export type ResearchPaper = {
  id: string;
  author_id: string;
  title: string;
  abstract: string;
  content: string;
  status: ResearchStatus;
  submitted_at: string | null;
  under_review_at: string | null;
  decided_at: string | null;
  published_at: string | null;
  elo_award: number | null;
  elo_awarded: boolean;
  elo_awarded_at: string | null;
  created_at: string;
  updated_at: string;
};

export type ResearchPage = {
  items: ResearchPaper[];
  limit: number;
  offset: number;
  has_more: boolean;
};

export type ResearchDraftInput = {
  title: string;
  abstract: string;
  content: string;
};

export type ResearchRevisionInput = ResearchDraftInput & {
  new_paper_id: string;
};

export type ResearchRubricScores = {
  relevance: number;
  methodology: number;
  evidence: number;
  originality: number;
  clarity_and_reproducibility: number;
};

export type ResearchEvidence = {
  reference: string;
  finding: string;
};

export type ResearchEvaluation = {
  rubric_version: 1;
  evaluated_content_version: number;
  scores: ResearchRubricScores;
  overall_score: number;
  recommendation: "approve" | "reject";
  rationale: string;
  evidence: ResearchEvidence[];
  strengths: string[];
  concerns: string[];
};

export type ResearchReviewInput = {
  score: number;
  recommendation: "approve" | "reject";
  comments?: string;
  evaluation: ResearchEvaluation;
};

export type ResearchReview = {
  score: number | null;
  recommendation: string;
  comments: string | null;
  evaluation: ResearchEvaluation | null;
  reviewed_at: string;
};

export type ResearchReviewsResponse = {
  reviews: ResearchReview[];
};

export type ResearchPageParams = {
  limit?: number;
  offset?: number;
};
