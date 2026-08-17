import { useCompleteLesson, useLearningCourse, useLearningProgress } from "./hooks";
import {
  formatLessonDuration,
  lessonNavigation,
  lessonPath,
  lessonProgress,
  type LearningCourse,
  type LearningLesson,
  type LearningProgressQueryState,
  type LearningProgressResponse,
} from "./types";
import "./LearningPage.css";

type LessonPageProps = Readonly<{
  lessonId?: string;
  courseId?: string;
}>;

function lessonIdFromLocation(): string | undefined {
  if (typeof window === "undefined") return undefined;
  const match = window.location.pathname.match(/^\/learning\/lessons\/([^/]+)$/);
  if (!match) return undefined;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return undefined;
  }
}

function LoadingLessonState() {
  return (
    <main aria-busy="true" aria-live="polite" className="learning-page">
      <p className="learning-page__eyebrow">Beginner learning</p>
      <h1>Loading lesson…</h1>
      <p role="status">Preparing the published lesson.</p>
    </main>
  );
}

function LessonErrorState({ onRetry }: { onRetry: () => void }) {
  return (
    <main className="learning-page" role="alert">
      <p className="learning-page__eyebrow">Beginner learning</p>
      <h1>Lesson unavailable</h1>
      <p>We could not load this lesson. Your saved progress was not changed.</p>
      <button type="button" onClick={() => void onRetry()}>Try again</button>
    </main>
  );
}

function SavedProgress({
  lesson,
  progressQuery,
}: {
  lesson: LearningLesson;
  progressQuery: LearningProgressQueryState;
}) {
  if (progressQuery.isPending && !progressQuery.data) {
    return <p className="learning-lesson__progress" role="status">Loading saved progress…</p>;
  }
  if (progressQuery.isError && !progressQuery.data) {
    return (
      <aside className="learning-lesson__progress learning-state--error" role="alert">
        <p>Saved progress is unavailable. No completion state has been assumed.</p>
        <button
          disabled={progressQuery.isFetching}
          type="button"
          onClick={() => void progressQuery.refetch()}
        >
          {progressQuery.isFetching ? "Trying again…" : "Try again"}
        </button>
      </aside>
    );
  }

  const item = lessonProgress(progressQuery.data, lesson.id);
  const savedState = item?.completed
    ? "Completed on the server."
    : item?.state === "started"
      ? "In progress on the server."
      : "Not started yet.";
  if (item?.completed) {
    return (
      <>
        {progressQuery.isError ? <ProgressRefreshError progressQuery={progressQuery} /> : null}
        <p className="learning-lesson__progress" role="status">{savedState}</p>
      </>
    );
  }
  return (
    <>
      {progressQuery.isError ? <ProgressRefreshError progressQuery={progressQuery} /> : null}
      <p className="learning-lesson__progress" role="status">{savedState}</p>
    </>
  );
}

function ProgressRefreshError({ progressQuery }: { progressQuery: LearningProgressQueryState }) {
  return (
    <aside className="learning-lesson__progress learning-state--error" role="alert">
      <p>Saved progress could not be refreshed. Showing your last saved state.</p>
      <button
        disabled={progressQuery.isFetching}
        type="button"
        onClick={() => void progressQuery.refetch()}
      >
        {progressQuery.isFetching ? "Trying again…" : "Try again"}
      </button>
    </aside>
  );
}

function LessonBody({ content }: { content: string }) {
  if (!content.trim()) {
    return <p className="learning-lesson__empty" role="status">This lesson has no published content yet.</p>;
  }
  const paragraphs = content.split(/\n\s*\n/).map((paragraph) => paragraph.trim()).filter(Boolean);
  return (
    <div className="learning-lesson__body">
      {(paragraphs.length > 0 ? paragraphs : [content]).map((paragraph, index) => (
        <p key={`${index}-${paragraph.slice(0, 20)}`}>{paragraph}</p>
      ))}
    </div>
  );
}

function CompletionControl({
  lesson,
  savedProgress,
}: {
  lesson: LearningLesson;
  savedProgress: LearningProgressResponse | undefined;
}) {
  const completion = useCompleteLesson();
  const confirmedProgress = completion.data?.progress ?? lessonProgress(savedProgress, lesson.id);

  if (confirmedProgress?.completed) {
    return <p className="learning-lesson__completion" role="status">Completion saved.</p>;
  }

  return (
    <div className="learning-lesson__completion-control">
      <button type="button" disabled={completion.isPending} onClick={() => completion.mutate(lesson.id)}>
        {completion.isPending ? "Saving completion…" : "Mark lesson complete"}
      </button>
      {completion.isError ? (
        <p className="learning-lesson__completion" role="alert">
          We could not save completion. Your previous progress remains unchanged.
        </p>
      ) : completion.isSuccess ? (
        <p className="learning-lesson__completion" role="status">Completion response received.</p>
      ) : null}
    </div>
  );
}

function LessonNavigation({
  course,
  lessonId,
}: {
  course: LearningCourse;
  lessonId: string;
}) {
  const { previous, next } = lessonNavigation(course, lessonId);
  return (
    <nav aria-label="Previous and next lessons" className="learning-lesson__navigation">
      {previous ? (
        <a
          aria-label={`Previous lesson: ${previous.title}`}
          href={lessonPath(previous.id)}
          rel="prev"
        >
          ← Previous: {previous.title}
        </a>
      ) : <span aria-disabled="true">Previous lesson</span>}
      {next ? (
        <a
          aria-label={`Next lesson: ${next.title}`}
          href={lessonPath(next.id)}
          rel="next"
        >
          Next: {next.title} →
        </a>
      ) : <span aria-disabled="true">Next lesson</span>}
    </nav>
  );
}

export function LessonView({
  course,
  lesson,
  progressQuery,
}: {
  course: LearningCourse;
  lesson: LearningLesson;
  progressQuery: LearningProgressQueryState;
}) {
  return (
    <main className="learning-page learning-lesson-page" aria-labelledby="learning-lesson-title">
      <nav aria-label="Lesson navigation" className="learning-breadcrumbs">
        <a href="/learning">Back to course</a>
      </nav>
      <article>
        <p className="learning-page__eyebrow">Lesson · {formatLessonDuration(lesson.estimated_minutes)}</p>
        <h1 id="learning-lesson-title">{lesson.title}</h1>
        {lesson.summary ? <p className="learning-page__description">{lesson.summary}</p> : null}
        <SavedProgress lesson={lesson} progressQuery={progressQuery} />
        <LessonBody content={lesson.content} />
        <CompletionControl lesson={lesson} savedProgress={progressQuery.data} />
        <LessonNavigation course={course} lessonId={lesson.id} />
      </article>
    </main>
  );
}

export function LessonPage({ lessonId, courseId }: LessonPageProps) {
  const courseQuery = useLearningCourse(courseId);
  const progressQuery = useLearningProgress();
  const selectedLessonId = lessonId ?? lessonIdFromLocation();

  if (courseQuery.isPending && !courseQuery.data) return <LoadingLessonState />;
  if (courseQuery.isError && !courseQuery.data) {
    return <LessonErrorState onRetry={() => courseQuery.refetch()} />;
  }
  if (!courseQuery.data) return <LoadingLessonState />;

  const lessons = courseQuery.data.modules.flatMap((module) => module.lessons);
  if (lessons.length === 0) {
    return (
      <main className="learning-page" aria-labelledby="learning-empty-title">
        <p className="learning-page__eyebrow">Beginner learning</p>
        <h1 id="learning-empty-title">No published lessons yet</h1>
        <p role="status">This course has no published lessons available right now.</p>
        <a href="/learning">Return to the course</a>
      </main>
    );
  }

  const lesson = lessons.find((candidate) => candidate.id === selectedLessonId);

  if (!lesson) {
    return (
      <main className="learning-page" role="alert">
        <p className="learning-page__eyebrow">Beginner learning</p>
        <h1>Lesson not found</h1>
        <p>This lesson is not part of the currently published course.</p>
        <a href="/learning">Return to the course</a>
      </main>
    );
  }

  return <LessonView course={courseQuery.data} lesson={lesson} progressQuery={progressQuery} />;
}
