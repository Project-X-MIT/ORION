import { apiClient } from "../../shared/api/client";

import {
  BEGINNER_COURSE_ID,
  type LearningCourse,
  type LessonCompletionResponse,
  type LearningProgressResponse,
} from "./types";

export function getLearningCourse(
  courseId = BEGINNER_COURSE_ID,
  signal?: AbortSignal,
): Promise<LearningCourse> {
  const options = signal ? { signal } : undefined;
  return apiClient.get<LearningCourse>(`/learning/courses/${encodeURIComponent(courseId)}`, options);
}

export function getLearningProgress(signal?: AbortSignal): Promise<LearningProgressResponse> {
  return signal
    ? apiClient.get<LearningProgressResponse>("/learning/progress", { signal })
    : apiClient.get<LearningProgressResponse>("/learning/progress");
}

export function completeLearningLesson(lessonId: string): Promise<LessonCompletionResponse> {
  return apiClient.post<LessonCompletionResponse>(
    `/learning/lessons/${encodeURIComponent(lessonId)}/completion`,
  );
}
