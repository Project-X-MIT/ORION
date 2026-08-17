import { useRef, useState } from "react";

import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { isApiClientError } from "../../shared/api/errors";
import { useSubmitQuiz } from "./hooks";
import type {
  BasicSubmission,
  BasicSubmitRequest,
  QuizQuestion,
} from "./types";

type BasicQuizProps = {
  questions: QuizQuestion[];
  onChooseDifferentMode: () => void;
};

function createAttemptId(): string {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  const bytes = new Uint8Array(16);
  if (globalThis.crypto?.getRandomValues) {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) bytes[index] = Math.floor(Math.random() * 256);
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function ratingDelta(submission: BasicSubmission): number {
  return submission.answers.reduce((total, answer) => total + answer.rating_delta, 0);
}

function formatDelta(value: number): string {
  return value > 0 ? `+${value}` : `${value}`;
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "The quiz could not be submitted. Please try again.";
}

function BasicResult({ submission, onRestart, onChooseDifferentMode }: {
  submission: BasicSubmission;
  onRestart: () => void;
  onChooseDifferentMode: () => void;
}) {
  return (
    <Card as="section" className="quiz-result" data-testid="quiz-result" aria-live="polite">
      <h3>Basic Quiz results</h3>
      <p><strong>Server score:</strong> {submission.attempt.score}%</p>
      <p><strong>Correct answers:</strong> {submission.attempt.correct_answers} of {submission.attempt.total_questions}</p>
      <p><strong>Rating change:</strong> {formatDelta(ratingDelta(submission))}</p>
      <p><strong>Server rating:</strong> {submission.rating.rating}</p>
      <div className="quiz-actions">
        <Button className="quiz-action" onClick={onRestart}>Try another Basic Quiz</Button>
        <Button className="quiz-action" variant="secondary" onClick={onChooseDifferentMode}>
          Choose another mode
        </Button>
      </div>
    </Card>
  );
}

export function BasicQuiz({ questions, onChooseDifferentMode }: BasicQuizProps) {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [startedAt, setStartedAt] = useState(() => new Date().toISOString());
  const [validationError, setValidationError] = useState<string | null>(null);
  const [submission, setSubmission] = useState<BasicSubmission | null>(null);
  const submitQuiz = useSubmitQuiz();
  const submittingRef = useRef(false);
  const attemptIdRef = useRef(createAttemptId());
  const payloadRef = useRef<BasicSubmitRequest | null>(null);

  function resetSubmissionAttempt() {
    attemptIdRef.current = createAttemptId();
    payloadRef.current = null;
  }

  function restart() {
    setAnswers({});
    setStartedAt(new Date().toISOString());
    setValidationError(null);
    setSubmission(null);
    resetSubmissionAttempt();
  }

  async function handleSubmit() {
    if (submittingRef.current || submitQuiz.isPending || submission) return;

    const unanswered = questions.find((question) => !answers[question.id]);
    if (unanswered) {
      setValidationError("Answer every question before submitting.");
      document.getElementById(`question-${unanswered.id}`)?.focus();
      return;
    }

    const payload = payloadRef.current ?? {
      attempt_id: attemptIdRef.current,
      answers: questions.map((question) => ({
        question_id: question.id,
        option_id: answers[question.id],
      })),
      started_at: startedAt,
      completed_at: new Date().toISOString(),
    };
    payloadRef.current = payload;
    submittingRef.current = true;
    setValidationError(null);
    try {
      const result = await submitQuiz.mutateAsync({
        mode: "basic",
        payload,
      });
      if ("answers" in result) {
        setSubmission(result);
      } else {
        setValidationError("The server returned an unexpected Basic Quiz result.");
        resetSubmissionAttempt();
      }
    } catch (error) {
      setValidationError(getErrorMessage(error));
      if (!isApiClientError(error) || !error.isRetryable) resetSubmissionAttempt();
    } finally {
      submittingRef.current = false;
    }
  }

  const retryLocked = Boolean(payloadRef.current && validationError);

  if (submission) {
    return (
      <BasicResult
        submission={submission}
        onRestart={restart}
        onChooseDifferentMode={onChooseDifferentMode}
      />
    );
  }

  return (
    <Card as="section" className="quiz-panel" aria-labelledby="basic-quiz-heading">
      <div className="quiz-panel-header">
        <div>
          <h2 id="basic-quiz-heading">Basic Quiz</h2>
          <p>{questions.length} multiple-choice question{questions.length === 1 ? "" : "s"}.</p>
        </div>
        <span className="quiz-status">Your answers are not scored in the browser.</span>
      </div>

      <div className="quiz-question-list">
        {questions.map((question, index) => (
          <fieldset className="quiz-question" key={question.id}>
            <legend id={`question-${question.id}`} tabIndex={-1}>
              <span className="quiz-category">Question {index + 1} · {question.category}</span>
              {question.question_text}
            </legend>
            <div className="quiz-options">
              {question.options
                .slice()
                .sort((left, right) => left.position - right.position)
                .map((option) => (
                  <label className="quiz-option" key={option.id}>
                    <input
                      type="radio"
                      name={`question-${question.id}`}
                      value={option.id}
                      checked={answers[question.id] === option.id}
                      disabled={submitQuiz.isPending || retryLocked}
                      onChange={() => {
                        setAnswers((current) => ({ ...current, [question.id]: option.id }));
                        setValidationError(null);
                      }}
                    />
                    <span>{option.option_text}</span>
                  </label>
                ))}
            </div>
          </fieldset>
        ))}
      </div>

      {validationError ? <div className="quiz-error" role="alert">{validationError}</div> : null}
      <div className="quiz-actions">
        <Button
          className="quiz-action"
          data-testid="quiz-submit"
          disabled={submitQuiz.isPending}
          isLoading={submitQuiz.isPending}
          loadingLabel="Submitting Basic Quiz"
          aria-busy={submitQuiz.isPending}
          onClick={() => void handleSubmit()}
        >
          {payloadRef.current && validationError ? "Retry submission" : "Submit Basic Quiz"}
        </Button>
        <Button className="quiz-action" variant="secondary" onClick={onChooseDifferentMode}>
          Back to modes
        </Button>
      </div>
    </Card>
  );
}
