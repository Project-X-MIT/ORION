import { useLearningCourse, useLearningProgress } from "./hooks";
import {
  formatLessonDuration,
  lessonPath,
  lessonProgress,
  resumeLesson,
  type LearningCourse,
  type LearningModule,
  type LearningProgressQueryState,
  type LearningProgressResponse,
} from "./types";
import "./LearningPage.css";

type LearningPageProps = Readonly<{
  courseId?: string;
}>;

function LoadingState() {
  return (
    <main aria-busy="true" aria-live="polite" className="learning-page">
      <p className="learning-page__eyebrow">Beginner learning</p>
      <h1>Loading your course…</h1>
      <p role="status">Preparing the published lessons.</p>
    </main>
  );
}

function CourseErrorState({ onRetry }: { onRetry: () => void }) {
  return (
    <main className="learning-page" role="alert">
      <p className="learning-page__eyebrow">Beginner learning</p>
      <h1>Course unavailable</h1>
      <p>We could not load the published course. Your saved progress was not changed.</p>
      <button type="button" onClick={() => void onRetry()}>Try again</button>
    </main>
  );
}

function ProgressSummary({ progressQuery }: { progressQuery: LearningProgressQueryState }) {
  const { data: progress } = progressQuery;
  if (progressQuery.isPending && !progress) {
    return <p className="learning-progress" role="status">Loading saved progress…</p>;
  }
  if (progressQuery.isError && !progress) {
    return (
      <aside className="learning-progress learning-state--error" role="alert">
        <p>Saved progress is unavailable. Course content is still available.</p>
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
  if (!progress) {
    return <p className="learning-progress" role="status">Saved progress is unavailable.</p>;
  }

  const { completed_lessons: completedLessons, total_lessons: totalLessons } = progress.summary;
  return (
    <>
      {progressQuery.isError ? (
        <aside className="learning-progress learning-state--error" role="alert">
          <p>Saved progress could not be refreshed. Showing your last saved state.</p>
          <button
            disabled={progressQuery.isFetching}
            type="button"
            onClick={() => void progressQuery.refetch()}
          >
            {progressQuery.isFetching ? "Trying again…" : "Try again"}
          </button>
        </aside>
      ) : null}
      <p className="learning-progress" aria-label="Course progress">
        {completedLessons} of {totalLessons} lessons completed
        {progressQuery.isFetching ? " · Refreshing saved progress…" : ""}
      </p>
    </>
  );
}

function CourseRefreshState({
  isError,
  isFetching,
  onRetry,
}: {
  isError: boolean;
  isFetching: boolean;
  onRetry: () => unknown;
}) {
  if (isError) {
    return (
      <aside className="learning-state learning-state--error" role="alert">
        <strong>Course refresh failed.</strong>
        <p>Showing the last available published course.</p>
        <button disabled={isFetching} type="button" onClick={() => void onRetry()}>
          {isFetching ? "Trying again…" : "Try again"}
        </button>
      </aside>
    );
  }
  return isFetching ? (
    <p className="learning-state" role="status">Refreshing published course…</p>
  ) : null;
}

function LessonLink({
  lesson,
  progress,
}: {
  lesson: LearningModule["lessons"][number];
  progress: LearningProgressResponse | undefined;
}) {
  const item = lessonProgress(progress, lesson.id);
  return (
    <li className="learning-lesson-link">
      <a href={lessonPath(lesson.id)}>
        <span className="learning-lesson-link__title">{lesson.title}</span>
        <span className="learning-lesson-link__meta">
          {formatLessonDuration(lesson.estimated_minutes)}
          {item?.completed ? " · Completed" : ""}
        </span>
      </a>
      {lesson.summary ? <p>{lesson.summary}</p> : null}
    </li>
  );
}

function ModuleCard({
  module,
  progress,
}: {
  module: LearningModule;
  progress: LearningProgressResponse | undefined;
}) {
  return (
    <li className="learning-module">
      <article aria-labelledby={`learning-module-${module.id}`}>
        <p className="learning-module__number">Module {module.display_order}</p>
        <h2 id={`learning-module-${module.id}`}>{module.title}</h2>
        {module.description ? <p>{module.description}</p> : null}
        {module.lessons.length > 0 ? (
          <ol className="learning-lesson-list" aria-label={`${module.title} lessons`}>
            {module.lessons.map((lesson) => (
              <LessonLink key={lesson.id} lesson={lesson} progress={progress} />
            ))}
          </ol>
        ) : (
          <p role="status">No published lessons are available in this module.</p>
        )}
      </article>
    </li>
  );
}

export function CourseLanding({
  course,
  progressQuery,
  courseQuery,
}: {
  course: LearningCourse;
  progressQuery: LearningProgressQueryState;
  courseQuery: {
    isError: boolean;
    isFetching: boolean;
    refetch: () => unknown;
  };
}) {
  const lessonToResume = resumeLesson(course, progressQuery.data);

  return (
    <main className="learning-page" aria-labelledby="learning-course-title">
      <header className="learning-page__hero">
        <CourseRefreshState
          isError={courseQuery.isError}
          isFetching={courseQuery.isFetching}
          onRetry={courseQuery.refetch}
        />
        <p className="learning-page__eyebrow">Beginner learning · Version {course.version}</p>
        <h1 id="learning-course-title">{course.title}</h1>
        <p className="learning-page__description">{course.description}</p>
        <ProgressSummary progressQuery={progressQuery} />
        {lessonToResume ? (
          <p className="learning-continue">
            <a href={lessonPath(lessonToResume.id)}>Continue with {lessonToResume.title}</a>
          </p>
        ) : progressQuery.data?.summary.completed ? (
          <p className="learning-continue" role="status">You have completed this course.</p>
        ) : null}
      </header>
      <section aria-labelledby="learning-modules-title">
        <div className="learning-section-heading">
          <h2 id="learning-modules-title">Course modules</h2>
          <p>Work through the lessons in order and return whenever you are ready.</p>
        </div>
        {course.modules.length > 0 ? (
          <ol className="learning-module-list">
            {course.modules.map((module) => (
              <ModuleCard key={module.id} module={module} progress={progressQuery.data} />
            ))}
          </ol>
        ) : (
          <p role="status">No published modules are available yet.</p>
        )}
      </section>
    </main>
  );
}

export function LearningPage({ courseId }: LearningPageProps) {
  const courseQuery = useLearningCourse(courseId);
  const progressQuery = useLearningProgress();

  if (courseQuery.isPending && !courseQuery.data) return <LoadingState />;
  if (courseQuery.isError && !courseQuery.data) {
    return <CourseErrorState onRetry={() => courseQuery.refetch()} />;
  }
  if (!courseQuery.data) return <LoadingState />;

  return (
    <CourseLanding
      course={courseQuery.data}
      courseQuery={{
        isError: courseQuery.isError,
        isFetching: courseQuery.isFetching,
        refetch: courseQuery.refetch,
      }}
      progressQuery={{
        data: progressQuery.data,
        isPending: progressQuery.isPending,
        isError: progressQuery.isError,
        isFetching: progressQuery.isFetching,
        refetch: progressQuery.refetch,
      }}
    />
  );
}
