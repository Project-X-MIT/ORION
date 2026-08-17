import { renderToStaticMarkup } from "react-dom/server";
import type { UseQueryResult } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { LearningPage } from "./LearningPage";
import { LessonPage } from "./LessonPage";
import { useCompleteLesson, useLearningCourse, useLearningProgress } from "./hooks";
import type { LearningCourse, LearningProgressResponse } from "./types";

vi.mock("./hooks", () => ({
  useCompleteLesson: vi.fn(),
  useLearningCourse: vi.fn(),
  useLearningProgress: vi.fn(),
}));

const mockedUseCompleteLesson = vi.mocked(useCompleteLesson);
const mockedUseLearningCourse = vi.mocked(useLearningCourse);
const mockedUseLearningProgress = vi.mocked(useLearningProgress);

const course: LearningCourse = {
  id: "course-1",
  slug: "beginner-trading",
  title: "Beginner Trading",
  description: "Learn the foundations of markets and responsible trading.",
  version: 1,
  modules: [
    {
      id: "module-1",
      slug: "foundations",
      title: "Market Foundations",
      description: "Start with the basics.",
      display_order: 1,
      lessons: [
        {
          id: "lesson-1",
          module_id: "module-1",
          slug: "what-is-a-market",
          title: "What Is a Market?",
          summary: "Understand buyers and sellers.",
          content: "A market brings buyers and sellers together.\n\n<script>alert(1)</script>",
          lesson_order: 1,
          estimated_minutes: 8,
        },
      ],
    },
  ],
};

function courseWithThreeLessons(): LearningCourse {
  const firstLesson = course.modules[0].lessons[0];
  return {
    ...course,
    modules: [{
      ...course.modules[0],
      lessons: [
        firstLesson,
        {
          ...firstLesson,
          id: "lesson-2",
          slug: "reading-a-chart",
          title: "Reading a Chart",
          lesson_order: 2,
        },
        {
          ...firstLesson,
          id: "lesson-3",
          slug: "market-risk",
          title: "Market Risk",
          lesson_order: 3,
        },
      ],
    }],
  };
}

const progress: LearningProgressResponse = {
  items: [
    {
      lesson_id: "lesson-1",
      state: "completed",
      completed: true,
      started_at: "2026-08-16T08:00:00Z",
      completed_at: "2026-08-16T08:10:00Z",
      last_accessed_at: "2026-08-16T08:10:00Z",
      updated_at: "2026-08-16T08:10:00Z",
    },
  ],
  summary: {
    total_modules: 1,
    completed_modules: 1,
    total_lessons: 1,
    completed_lessons: 1,
    completed: true,
  },
};

function queryResult<T>(data: T | undefined, overrides: Record<string, unknown> = {}): UseQueryResult<T, Error> {
  return {
    data,
    isPending: false,
    isError: false,
    refetch: vi.fn(),
    ...overrides,
  } as unknown as UseQueryResult<T, Error>;
}

function completionResult(overrides: Record<string, unknown> = {}): ReturnType<typeof useCompleteLesson> {
  return {
    data: undefined,
    error: null,
    isError: false,
    isIdle: true,
    isPending: false,
    isSuccess: false,
    mutate: vi.fn(),
    mutateAsync: vi.fn(),
    reset: vi.fn(),
    ...overrides,
  } as unknown as ReturnType<typeof useCompleteLesson>;
}

describe("learning pages", () => {
  beforeEach(() => {
    mockedUseCompleteLesson.mockReset();
    mockedUseLearningCourse.mockReset();
    mockedUseLearningProgress.mockReset();
    mockedUseCompleteLesson.mockReturnValue(completionResult());
  });

  it("renders the course landing, ordered module, lesson link, and confirmed progress", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("Beginner Trading");
    expect(markup).toContain("Market Foundations");
    expect(markup).toContain('href="/learning/lessons/lesson-1"');
    expect(markup).toContain("1 of 1");
    expect(markup).toContain("Completed");
  });

  it("renders a lesson without interpreting content as HTML", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("What Is a Market?");
    expect(markup).toContain("A market brings buyers and sellers together.");
    expect(markup).toContain("Completed on the server.");
    expect(markup).toContain("Completion saved.");
    expect(markup).not.toContain("<script>");
    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
  });

  it("renders an accessible loading state", () => {
    mockedUseLearningCourse.mockReturnValue(
      queryResult<LearningCourse>(undefined, { data: undefined, isPending: true }),
    );
    mockedUseLearningProgress.mockReturnValue(
      queryResult<LearningProgressResponse>(undefined, { data: undefined, isPending: true }),
    );

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain('aria-busy="true"');
    expect(markup).toContain("Loading your course");
  });

  it("shows progress loading without inventing a resume target", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: true,
      isFetching: true,
      isError: false,
    }));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("Loading saved progress");
    expect(markup).not.toContain("Continue with");
  });

  it("renders a recoverable error without inventing progress", () => {
    const refetch = vi.fn();
    mockedUseLearningCourse.mockReturnValue(queryResult<LearningCourse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
      refetch,
    }));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
    }));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("Course unavailable");
    expect(markup).toContain("Your saved progress was not changed.");
    expect(markup).toContain("Try again");
    expect(markup).not.toContain("Completed");
  });

  it("keeps the course available when progress can be retried", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
      isFetching: false,
      refetch: vi.fn(),
    }));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("Beginner Trading");
    expect(markup).toContain("Saved progress is unavailable");
    expect(markup).toContain("Course content is still available");
    expect(markup).toContain("Try again");
  });

  it("renders the empty course state", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult({ ...course, modules: [] }));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("No published modules are available yet.");
  });

  it("keeps cached course content visible after a refresh error", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course, {
      isError: true,
      isFetching: false,
      refetch: vi.fn(),
    }));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain("Course refresh failed.");
    expect(markup).toContain("Showing the last available published course.");
    expect(markup).toContain("Beginner Trading");
  });

  it("renders a recoverable lesson loading state", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult<LearningCourse>(undefined, {
      data: undefined,
      isPending: true,
      isError: false,
    }));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("Loading lesson");
    expect(markup).toContain("Preparing the published lesson.");
  });

  it("renders a retryable lesson error", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult<LearningCourse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
      refetch: vi.fn(),
    }));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("Lesson unavailable");
    expect(markup).toContain("Try again");
    expect(markup).toContain("Your saved progress was not changed.");
  });

  it("renders an empty lesson state when no lessons are published", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult({ ...course, modules: [] }));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("No published lessons yet");
    expect(markup).toContain("This course has no published lessons available right now.");
    expect(markup).toContain("Return to the course");
  });

  it("does not mark a lesson complete when saved progress failed to load", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
    }));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("Saved progress is unavailable");
    expect(markup).toContain("No completion state has been assumed");
    expect(markup).not.toContain("Completed on the server.");
    expect(markup).toContain("Mark lesson complete");
    expect(markup).toContain("Try again");
  });

  it("resumes the most recently accessed incomplete lesson", () => {
    const resumableCourse: LearningCourse = {
      ...course,
      modules: [{
        ...course.modules[0],
        lessons: [
          course.modules[0].lessons[0],
          {
            ...course.modules[0].lessons[0],
            id: "lesson-2",
            slug: "reading-a-chart",
            title: "Reading a Chart",
            lesson_order: 2,
          },
        ],
      }],
    };
    const resumableProgress: LearningProgressResponse = {
      items: [{
        lesson_id: "lesson-2",
        state: "started",
        completed: false,
        started_at: "2026-08-16T08:00:00Z",
        completed_at: null,
        last_accessed_at: "2026-08-16T08:20:00Z",
        updated_at: "2026-08-16T08:20:00Z",
      }],
      summary: {
        total_modules: 1,
        completed_modules: 0,
        total_lessons: 2,
        completed_lessons: 0,
        completed: false,
      },
    };
    mockedUseLearningCourse.mockReturnValue(queryResult(resumableCourse));
    mockedUseLearningProgress.mockReturnValue(queryResult(resumableProgress));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain('href="/learning/lessons/lesson-2"');
    expect(markup).toContain("Continue with Reading a Chart");
  });

  it("resumes at the authoritative next lesson after completion", () => {
    const resumableCourse = courseWithThreeLessons();
    const completedProgress: LearningProgressResponse = {
      items: [{
        lesson_id: "lesson-1",
        state: "completed",
        completed: true,
        started_at: "2026-08-16T08:00:00Z",
        completed_at: "2026-08-16T08:10:00Z",
        last_accessed_at: "2026-08-16T08:10:00Z",
        updated_at: "2026-08-16T08:10:00Z",
      }],
      summary: {
        total_modules: 1,
        completed_modules: 0,
        total_lessons: 3,
        completed_lessons: 1,
        completed: false,
      },
    };
    mockedUseLearningCourse.mockReturnValue(queryResult(resumableCourse));
    mockedUseLearningProgress.mockReturnValue(queryResult(completedProgress));

    const markup = renderToStaticMarkup(<LearningPage />);

    expect(markup).toContain('href="/learning/lessons/lesson-2"');
    expect(markup).toContain("Continue with Reading a Chart");
  });

  it("does not show a completed state after a failed completion write", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
    }));
    mockedUseCompleteLesson.mockReturnValue(completionResult({
      error: new Error("write failed"),
      isError: true,
      isIdle: false,
      isSuccess: false,
    }));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("previous progress remains unchanged");
    expect(markup).not.toContain("Completion saved.");
  });

  it("shows completion only from the server completion response", () => {
    mockedUseLearningCourse.mockReturnValue(queryResult(course));
    mockedUseLearningProgress.mockReturnValue(queryResult<LearningProgressResponse>(undefined, {
      data: undefined,
      isPending: false,
      isError: true,
    }));
    mockedUseCompleteLesson.mockReturnValue(completionResult({
      data: { progress: progress.items[0] },
      isIdle: false,
      isSuccess: true,
    }));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain("Completion saved.");
    expect(markup).not.toContain("Mark lesson complete");
  });

  it("renders next navigation for the first lesson", () => {
    const navigationCourse = courseWithThreeLessons();
    mockedUseLearningCourse.mockReturnValue(queryResult(navigationCourse));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-1" />);

    expect(markup).toContain('href="/learning/lessons/lesson-2"');
    expect(markup).not.toContain('href="/learning/lessons/lesson-0"');
    expect(markup).toContain("Next lesson: Reading a Chart");
  });

  it("renders both neighbors for a middle lesson", () => {
    const navigationCourse = courseWithThreeLessons();
    mockedUseLearningCourse.mockReturnValue(queryResult(navigationCourse));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-2" />);

    expect(markup).toContain('href="/learning/lessons/lesson-1"');
    expect(markup).toContain('href="/learning/lessons/lesson-3"');
    expect(markup).toContain("Previous lesson: What Is a Market?");
    expect(markup).toContain("Next lesson: Market Risk");
  });

  it("renders previous navigation and an unavailable next state for the last lesson", () => {
    const navigationCourse = courseWithThreeLessons();
    mockedUseLearningCourse.mockReturnValue(queryResult(navigationCourse));
    mockedUseLearningProgress.mockReturnValue(queryResult(progress));

    const markup = renderToStaticMarkup(<LessonPage lessonId="lesson-3" />);

    expect(markup).toContain('href="/learning/lessons/lesson-2"');
    expect(markup).not.toContain('href="/learning/lessons/lesson-4"');
    expect(markup).toContain("Next lesson");
  });
});
