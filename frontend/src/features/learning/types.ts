export const BEGINNER_COURSE_ID = "00000000-0000-0000-0000-000000000001";

export type LearningLesson = Readonly<{
  id: string;
  module_id: string;
  slug: string;
  title: string;
  summary: string | null;
  content: string;
  lesson_order: number;
  estimated_minutes: number;
}>;

export type LearningModule = Readonly<{
  id: string;
  slug: string;
  title: string;
  description: string | null;
  display_order: number;
  lessons: readonly LearningLesson[];
}>;

export type LearningCourse = Readonly<{
  id: string;
  slug: string;
  title: string;
  description: string;
  version: number;
  modules: readonly LearningModule[];
}>;

export type ProgressState = "not_started" | "started" | "completed";

export type LearningProgressItem = Readonly<{
  lesson_id: string;
  state: ProgressState;
  completed: boolean;
  started_at: string | null;
  completed_at: string | null;
  last_accessed_at: string | null;
  updated_at: string;
}>;

export type LearningProgressSummary = Readonly<{
  total_modules: number;
  completed_modules: number;
  total_lessons: number;
  completed_lessons: number;
  completed: boolean;
}>;

export type LearningProgressResponse = Readonly<{
  items: readonly LearningProgressItem[];
  summary: LearningProgressSummary;
}>;

export type LearningProgressQueryState = Readonly<{
  data: LearningProgressResponse | undefined;
  isPending: boolean;
  isError: boolean;
  isFetching: boolean;
  refetch: () => unknown;
}>;

export type LessonCompletionResponse = Readonly<{
  progress: LearningProgressItem;
}>;

export type LessonNavigation = Readonly<{
  previous: LearningLesson | undefined;
  next: LearningLesson | undefined;
}>;

export function lessonPath(lessonId: string): string {
  return `/learning/lessons/${encodeURIComponent(lessonId)}`;
}

export function lessonProgress(
  progress: LearningProgressResponse | undefined,
  lessonId: string,
): LearningProgressItem | undefined {
  return progress?.items.find((item) => item.lesson_id === lessonId);
}

/** Returns the published course lessons in the API's stable display order. */
export function orderedLessons(course: LearningCourse): LearningLesson[] {
  return course.modules
    .flatMap((module) => module.lessons.map((lesson) => ({
      lesson,
      moduleOrder: module.display_order,
    })))
    .sort((left, right) =>
      left.moduleOrder - right.moduleOrder ||
      left.lesson.lesson_order - right.lesson.lesson_order ||
      left.lesson.id.localeCompare(right.lesson.id),
    )
    .map(({ lesson }) => lesson);
}

export function lessonNavigation(course: LearningCourse, lessonId: string): LessonNavigation {
  const lessons = orderedLessons(course);
  const index = lessons.findIndex((lesson) => lesson.id === lessonId);
  if (index < 0) return { previous: undefined, next: undefined };

  return {
    previous: lessons[index - 1],
    next: lessons[index + 1],
  };
}

/**
 * Selects the lesson to resume from server-owned progress. An in-progress
 * lesson is preferred by most recent access; ties use the course's published
 * order. If no lesson has started, the first incomplete lesson is returned.
 */
export function resumeLesson(
  course: LearningCourse,
  progress: LearningProgressResponse | undefined,
): LearningLesson | undefined {
  if (!progress) return undefined;

  const lessons = orderedLessons(course);
  const incomplete = lessons.filter((lesson) => !lessonProgress(progress, lesson.id)?.completed);
  const started = incomplete
    .map((lesson, index) => ({ lesson, index, item: lessonProgress(progress, lesson.id) }))
    .filter((entry): entry is { lesson: LearningLesson; index: number; item: LearningProgressItem } =>
      entry.item?.state === "started",
    )
    .sort((left, right) => {
      const leftParsed = left.item.last_accessed_at ? Date.parse(left.item.last_accessed_at) : Number.NaN;
      const rightParsed = right.item.last_accessed_at ? Date.parse(right.item.last_accessed_at) : Number.NaN;
      const leftAccess = Number.isFinite(leftParsed) ? leftParsed : Number.NEGATIVE_INFINITY;
      const rightAccess = Number.isFinite(rightParsed) ? rightParsed : Number.NEGATIVE_INFINITY;
      return rightAccess - leftAccess || left.index - right.index;
    });

  return started[0]?.lesson ?? incomplete[0];
}

export function formatLessonDuration(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) return "Duration unavailable";
  return `${minutes} minute${minutes === 1 ? "" : "s"}`;
}
