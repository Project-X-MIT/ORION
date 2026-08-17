import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import * as quizApi from "./api";
import type { QuizMode, QuizSubmission, QuizSubmissionInput } from "./types";

export const quizQueryKeys = {
  all: ["quiz"] as const,
  questions: (mode: QuizMode) => ["quiz", "questions", mode] as const,
  result: (attemptId: string) => ["quiz", "attempt", attemptId] as const,
};

export function useQuizQuestions(mode: QuizMode | null) {
  return useQuery({
    queryKey: mode ? quizQueryKeys.questions(mode) : quizQueryKeys.all,
    queryFn: ({ signal }) => quizApi.getQuizQuestions(mode as QuizMode, signal),
    enabled: mode !== null,
    retry: false,
  });
}

export function useSubmitQuiz() {
  const queryClient = useQueryClient();

  return useMutation<QuizSubmission, Error, QuizSubmissionInput>({
    mutationFn: (input: QuizSubmissionInput) =>
      input.mode === "basic"
        ? quizApi.submitBasicQuiz(input.payload)
        : quizApi.submitAdvancedQuiz(input.payload),
    retry: false,
    onSuccess: (_submission, input) => {
      void queryClient.invalidateQueries({
        queryKey: quizQueryKeys.questions(input.mode),
      });
    },
  });
}

export function useQuizAttemptResult(attemptId: string | null) {
  return useQuery({
    queryKey: attemptId ? quizQueryKeys.result(attemptId) : quizQueryKeys.all,
    queryFn: ({ signal }) => quizApi.getQuizAttemptResult(attemptId as string, signal),
    enabled: attemptId !== null,
    retry: false,
  });
}
