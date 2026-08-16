import { useState } from "react";

import { VisuallyHidden } from "../../shared/accessibility/VisuallyHidden";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { AdvancedQuiz } from "./AdvancedQuiz";
import { BasicQuiz } from "./BasicQuiz";
import { useQuizQuestions } from "./hooks";
import type { QuizMode } from "./types";

const QUIZ_STYLES = `
  .quiz-page { box-sizing: border-box; min-height: 100vh; padding: 2rem 1rem 4rem; color: #182230; background: #f7f9fc; }
  .quiz-page *, .quiz-page *::before, .quiz-page *::after { box-sizing: border-box; }
  .quiz-container { width: min(100%, 72rem); margin: 0 auto; }
  .quiz-header { margin-bottom: 2rem; }
  .quiz-eyebrow { margin: 0 0 .5rem; color: #3569d4; font-size: .78rem; font-weight: 700; letter-spacing: .1em; text-transform: uppercase; }
  .quiz-header h1 { margin: 0; font-size: clamp(1.8rem, 4vw, 3rem); line-height: 1.1; }
  .quiz-header p { max-width: 46rem; margin: .85rem 0 0; color: #506174; font-size: 1.05rem; line-height: 1.6; }
  .quiz-mode-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1rem; }
  .quiz-mode-card, .quiz-panel { border: 1px solid #d7e0eb; border-radius: 1rem; background: #fff; box-shadow: 0 .4rem 1.5rem rgba(27, 46, 75, .06); }
  .quiz-mode-card { padding: 1.5rem; color: inherit; text-align: left; cursor: pointer; transition: border-color .15s ease, transform .15s ease, box-shadow .15s ease; }
  .quiz-mode-card:hover, .quiz-mode-card:focus-visible { border-color: #3569d4; box-shadow: 0 .6rem 1.8rem rgba(53, 105, 212, .16); transform: translateY(-1px); }
  .quiz-mode-card:focus-visible, .quiz-action:focus-visible, .quiz-back:focus-visible, .quiz-option:focus-within { outline: 3px solid #8bb3ff; outline-offset: 3px; }
  .quiz-mode-card > .ui-card__body, .quiz-panel > .ui-card__body, .quiz-result > .ui-card__body { padding: 0; }
  .quiz-mode-card h2, .quiz-mode-card h3 { margin: 0 0 .5rem; font-size: 1.2rem; }
  .quiz-mode-card p { margin: 0; color: #506174; line-height: 1.5; }
  .quiz-mode-card span { display: inline-block; margin-top: 1rem; color: #3569d4; font-weight: 700; }
  .quiz-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: 1rem; }
  .quiz-back { border: 0; padding: .45rem 0; color: #3569d4; background: transparent; font: inherit; font-weight: 700; cursor: pointer; }
  .quiz-panel { padding: clamp(1rem, 3vw, 2rem); }
  .quiz-panel-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; margin-bottom: 1.5rem; }
  .quiz-panel-header h2 { margin: 0; font-size: clamp(1.35rem, 3vw, 2rem); }
  .quiz-panel-header p { margin: .35rem 0 0; color: #506174; }
  .quiz-question-list { display: grid; gap: 1.25rem; }
  .quiz-question { margin: 0; padding: 1rem; border: 1px solid #e0e7f0; border-radius: .75rem; }
  .quiz-question legend { width: 100%; padding: 0; font-weight: 700; line-height: 1.5; }
  .quiz-category { margin: 0 0 .35rem; color: #3569d4; font-size: .78rem; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .quiz-options { display: grid; gap: .65rem; margin-top: .9rem; }
  .quiz-option { display: flex; align-items: flex-start; gap: .7rem; padding: .8rem; border: 1px solid #d7e0eb; border-radius: .6rem; cursor: pointer; }
  .quiz-option:has(input:checked) { border-color: #3569d4; background: #f0f5ff; }
  .quiz-option input { width: 1.1rem; height: 1.1rem; margin-top: .1rem; accent-color: #3569d4; }
  .quiz-number-input { display: block; width: min(100%, 24rem); margin-top: .9rem; padding: .8rem .9rem; border: 1px solid #aebdce; border-radius: .6rem; color: inherit; font: inherit; }
  .quiz-number-input:focus { border-color: #3569d4; outline: 3px solid #c5d8ff; }
  .quiz-help { margin: .5rem 0 0; color: #627286; font-size: .9rem; }
  .quiz-error { margin: 1rem 0; padding: .8rem 1rem; border: 1px solid #e6a7a7; border-radius: .6rem; color: #8c1d1d; background: #fff5f5; }
  .quiz-actions { display: flex; flex-wrap: wrap; gap: .75rem; margin-top: 1.5rem; }
  .quiz-action { border: 0; border-radius: .6rem; padding: .75rem 1.1rem; color: #fff; background: #3569d4; font: inherit; font-weight: 700; cursor: pointer; }
  .quiz-action:hover:not(:disabled) { background: #2857ba; }
  .quiz-action:disabled { cursor: not-allowed; opacity: .6; }
  .quiz-action--secondary { color: #24405f; background: #e9eff7; }
  .quiz-action--secondary:hover:not(:disabled) { background: #dce6f2; }
  .quiz-result { margin-top: 1.5rem; padding: 1rem; border: 1px solid #b8d8c0; border-radius: .75rem; background: #f2fbf4; }
  .quiz-result h3 { margin: 0 0 .75rem; }
  .quiz-result p { margin: .35rem 0; }
  .quiz-pending { border-color: #c7d8f4; background: #f3f7ff; }
  .quiz-empty, .quiz-loading { padding: 2rem; border: 1px dashed #b7c6d8; border-radius: .75rem; color: #506174; background: #fff; text-align: center; }
  .quiz-status { color: #506174; }
  @media (max-width: 42rem) {
    .quiz-page { padding: 1.25rem .75rem 3rem; }
    .quiz-mode-grid { grid-template-columns: 1fr; }
    .quiz-panel-header, .quiz-toolbar { align-items: flex-start; flex-direction: column; }
    .quiz-action { width: 100%; }
  }
`;

export type QuizPageProps = {
  initialMode?: QuizMode;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "We could not load this quiz. Please try again.";
}

function ModeSelection({ onSelect }: { onSelect: (mode: QuizMode) => void }) {
  return (
    <section aria-labelledby="quiz-mode-heading">
      <VisuallyHidden><h2 id="quiz-mode-heading">Choose a quiz mode</h2></VisuallyHidden>
      <div className="quiz-mode-grid">
        <Card
          className="quiz-mode-card"
          data-testid="quiz-mode-basic"
          aria-labelledby="quiz-basic-mode-heading"
          onActivate={() => onSelect("basic")}
        >
          <h3 id="quiz-basic-mode-heading">Basic Quiz</h3>
          <p>Answer multiple-choice questions and see your authoritative score immediately.</p>
          <span>Start Basic Quiz →</span>
        </Card>
        <Card
          className="quiz-mode-card"
          data-testid="quiz-mode-advanced"
          aria-labelledby="quiz-advanced-mode-heading"
          onActivate={() => onSelect("advanced")}
        >
          <h3 id="quiz-advanced-mode-heading">Advanced Quiz</h3>
          <p>Make more involved predictions and see a pending state when settlement is delayed.</p>
          <span>Start Advanced Quiz →</span>
        </Card>
      </div>
    </section>
  );
}

export function QuizPage({ initialMode }: QuizPageProps = {}) {
  const [mode, setMode] = useState<QuizMode | null>(initialMode ?? null);
  const questionsQuery = useQuizQuestions(mode);

  return (
    <main className="quiz-page">
      <style>{QUIZ_STYLES}</style>
      <div className="quiz-container">
        <header className="quiz-header">
          <p className="quiz-eyebrow">ORION learning arena</p>
          <h1>Choose your quiz</h1>
          <p>Practice with accessible questions. Scores and rating changes always come from the server.</p>
        </header>

        {mode === null ? <ModeSelection onSelect={setMode} /> : null}

        {mode !== null ? (
          <>
            <div className="quiz-toolbar">
              <Button className="quiz-back" variant="ghost" onClick={() => setMode(null)}>
                ← Choose another mode
              </Button>
              <span className="quiz-status" aria-live="polite">
                {questionsQuery.isFetching ? "Refreshing questions…" : `${mode === "basic" ? "Basic" : "Advanced"} Quiz`}
              </span>
            </div>
            {questionsQuery.isPending ? (
              <div className="quiz-loading" aria-busy="true" role="status">Loading questions…</div>
            ) : questionsQuery.error ? (
              <div className="quiz-error" role="alert">
                <p>{errorMessage(questionsQuery.error)}</p>
                <Button className="quiz-action" onClick={() => void questionsQuery.refetch()}>
                  Try again
                </Button>
              </div>
            ) : questionsQuery.data?.items.length ? (
              mode === "basic" ? (
                <BasicQuiz questions={questionsQuery.data.items} onChooseDifferentMode={() => setMode(null)} />
              ) : (
                <AdvancedQuiz questions={questionsQuery.data.items} onChooseDifferentMode={() => setMode(null)} />
              )
            ) : (
              <div className="quiz-empty" role="status">There are no active questions in this mode yet.</div>
            )}
          </>
        ) : null}
      </div>
    </main>
  );
}
