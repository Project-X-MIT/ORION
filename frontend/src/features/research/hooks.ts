import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";

import {
  createResearchDraft,
  createResearchRevision,
  getResearchPaper,
  getResearchReviews,
  listOwnResearchDrafts,
  listPublishedResearch,
  listResearchReviewQueue,
  submitResearchPaper,
  submitResearchReview,
  updateResearchDraft,
} from "./api";
import type {
  ResearchDraftInput,
  ResearchPageParams,
  ResearchPaper,
  ResearchReviewInput,
  ResearchRevisionInput,
} from "./types";

const DEFAULT_PAGE: Required<ResearchPageParams> = { limit: 20, offset: 0 };

export const researchKeys = {
  all: ["research"] as const,
  published: (params: Required<ResearchPageParams>) => ["research", "published", params] as const,
  drafts: (params: Required<ResearchPageParams>) => ["research", "drafts", params] as const,
  reviewQueue: (params: Required<ResearchPageParams>) => ["research", "review-queue", params] as const,
  paper: (id: string) => ["research", "paper", id] as const,
  reviews: (id: string) => ["research", "reviews", id] as const,
};

function normalizePage(params?: ResearchPageParams): Required<ResearchPageParams> {
  return {
    limit: params?.limit ?? DEFAULT_PAGE.limit,
    offset: params?.offset ?? DEFAULT_PAGE.offset,
  };
}

export function usePublishedResearch(params?: ResearchPageParams) {
  const page = normalizePage(params);
  return useQuery({
    queryKey: researchKeys.published(page),
    queryFn: ({ signal }) => listPublishedResearch(page, signal),
  });
}

export function useOwnResearchDrafts(
  params?: ResearchPageParams,
  enabled = true,
) {
  const page = normalizePage(params);
  return useQuery({
    queryKey: researchKeys.drafts(page),
    queryFn: ({ signal }) => listOwnResearchDrafts(page, signal),
    enabled,
  });
}

export function useResearchPaper(id: string | undefined): UseQueryResult<ResearchPaper> {
  return useQuery({
    queryKey: researchKeys.paper(id ?? ""),
    queryFn: ({ signal }) => getResearchPaper(id as string, signal),
    enabled: Boolean(id),
  });
}

export function useResearchReviews(id: string | undefined) {
  return useQuery({
    queryKey: researchKeys.reviews(id ?? ""),
    queryFn: ({ signal }) => getResearchReviews(id as string, signal),
    enabled: Boolean(id),
  });
}

export function useResearchReviewQueue(
  params?: ResearchPageParams,
  enabled = true,
) {
  const page = normalizePage(params);
  return useQuery({
    queryKey: researchKeys.reviewQueue(page),
    queryFn: ({ signal }) => listResearchReviewQueue(page, signal),
    enabled,
  });
}

function invalidateResearchQueries(queryClient: ReturnType<typeof useQueryClient>, id?: string) {
  const requests = [queryClient.invalidateQueries({ queryKey: researchKeys.all })];
  if (id) requests.push(queryClient.invalidateQueries({ queryKey: researchKeys.paper(id) }));
  return Promise.all(requests);
}

export function useCreateResearchDraft() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: ResearchDraftInput) => createResearchDraft(input),
    onSuccess: (paper) => invalidateResearchQueries(queryClient, paper.id),
  });
}

export function useUpdateResearchDraft() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: ResearchDraftInput }) =>
      updateResearchDraft(id, input),
    onSuccess: (paper) => invalidateResearchQueries(queryClient, paper.id),
  });
}

export function useSubmitResearchPaper() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => submitResearchPaper(id),
    onSuccess: (paper) => invalidateResearchQueries(queryClient, paper.id),
  });
}

export function useCreateResearchRevision() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceId, input }: { sourceId: string; input: ResearchRevisionInput }) =>
      createResearchRevision(sourceId, input),
    onSuccess: (paper) => invalidateResearchQueries(queryClient, paper.id),
  });
}

export function useSubmitResearchReview() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: ResearchReviewInput }) =>
      submitResearchReview(id, input),
    onSuccess: (paper) => invalidateResearchQueries(queryClient, paper.id),
  });
}
