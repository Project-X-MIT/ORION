import { apiClient } from "../../shared/api/client";
import type {
  AdvancedSubmitRequest,
  AdvancedSubmission,
  AttemptResult,
  BasicSubmitRequest,
  BasicSubmission,
  QuestionPage,
  QuizMode,
} from "./types";

const QUESTION_PAGE_SIZE = 20;

export function getQuizQuestions(mode: QuizMode, signal?: AbortSignal): Promise<QuestionPage> {
  return apiClient.get<QuestionPage>(`/quiz/${mode}?limit=${QUESTION_PAGE_SIZE}&offset=0`, {
    signal,
  });
}

export function submitBasicQuiz(
  payload: BasicSubmitRequest,
): Promise<BasicSubmission> {
  return apiClient.post<BasicSubmission>("/quiz/basic/attempts", payload);
}

export function submitAdvancedQuiz(
  payload: AdvancedSubmitRequest,
): Promise<AdvancedSubmission> {
  return apiClient.post<AdvancedSubmission>("/quiz/advanced/attempts", payload);
}

export function getQuizAttemptResult(
  attemptId: string,
  signal?: AbortSignal,
): Promise<AttemptResult> {
  return apiClient.get<AttemptResult>(
    `/quiz/attempts/${encodeURIComponent(attemptId)}`,
    { signal },
  );
}
