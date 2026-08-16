import { apiClient } from "../../shared/api/client";
import type {
  ResearchDraftInput,
  ResearchEvaluation,
  ResearchPage,
  ResearchPageParams,
  ResearchPaper,
  ResearchReviewInput,
  ResearchReviewsResponse,
  ResearchRevisionInput,
} from "./types";

function pageQuery({ limit = 20, offset = 0 }: ResearchPageParams = {}): string {
  const query = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });
  return `?${query.toString()}`;
}

export function listPublishedResearch(
  params?: ResearchPageParams,
  signal?: AbortSignal,
): Promise<ResearchPage> {
  return apiClient.get<ResearchPage>(`/research${pageQuery(params)}`, { signal });
}

export function listOwnResearchDrafts(
  params?: ResearchPageParams,
  signal?: AbortSignal,
): Promise<ResearchPage> {
  return apiClient.get<ResearchPage>(`/research/drafts${pageQuery(params)}`, { signal });
}

export function listResearchReviewQueue(
  params?: ResearchPageParams,
  signal?: AbortSignal,
): Promise<ResearchPage> {
  return apiClient.get<ResearchPage>(`/research/review-queue${pageQuery(params)}`, { signal });
}

export function getResearchPaper(id: string, signal?: AbortSignal): Promise<ResearchPaper> {
  return apiClient.get<ResearchPaper>(`/research/${encodeURIComponent(id)}`, { signal });
}

export function getResearchReviews(
  id: string,
  signal?: AbortSignal,
): Promise<ResearchReviewsResponse> {
  return apiClient.get<ResearchReviewsResponse>(
    `/research/${encodeURIComponent(id)}/reviews`,
    { signal },
  );
}

export function createResearchDraft(
  input: ResearchDraftInput,
  signal?: AbortSignal,
): Promise<ResearchPaper> {
  return apiClient.post<ResearchPaper>("/research", input, { signal });
}

export function updateResearchDraft(
  id: string,
  input: ResearchDraftInput,
  signal?: AbortSignal,
): Promise<ResearchPaper> {
  return apiClient.put<ResearchPaper>(`/research/${encodeURIComponent(id)}`, input, { signal });
}

export function submitResearchPaper(id: string, signal?: AbortSignal): Promise<ResearchPaper> {
  return apiClient.post<ResearchPaper>(
    `/research/${encodeURIComponent(id)}/submission`,
    undefined,
    { signal },
  );
}

export function createResearchRevision(
  sourceId: string,
  input: ResearchRevisionInput,
  signal?: AbortSignal,
): Promise<ResearchPaper> {
  return apiClient.post<ResearchPaper>(
    `/research/${encodeURIComponent(sourceId)}/revisions`,
    input,
    { signal },
  );
}

export function submitResearchReview(
  id: string,
  input: ResearchReviewInput,
  signal?: AbortSignal,
): Promise<ResearchPaper> {
  return apiClient.post<ResearchPaper>(
    `/research/${encodeURIComponent(id)}/reviews`,
    input,
    { signal },
  );
}

export type { ResearchEvaluation };
