import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { completeLearningLesson, getLearningCourse, getLearningProgress } from "./api";
import { BEGINNER_COURSE_ID } from "./types";

export function useLearningCourse(courseId = BEGINNER_COURSE_ID) {
  return useQuery({
    queryKey: ["learning", "course", courseId],
    queryFn: ({ signal }) => getLearningCourse(courseId, signal),
  });
}

export function useLearningProgress() {
  return useQuery({
    queryKey: ["learning", "progress"],
    queryFn: ({ signal }) => getLearningProgress(signal),
  });
}

export function useCompleteLesson() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: completeLearningLesson,
    // Progress is deliberately not updated optimistically. PostgreSQL's
    // response is authoritative, including after a retry or another device.
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["learning", "progress"] });
    },
  });
}
