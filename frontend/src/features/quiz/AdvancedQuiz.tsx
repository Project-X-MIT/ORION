import { useRef, useState } from "react";

import { isApiClientError } from "../../shared/api/errors";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { NumberField } from "../../shared/forms/NumberField";
import { useSubmitQuiz } from "./hooks";
import type {
  AdvancedSubmission,
  AdvancedSubmitRequest,
  QuizQuestion,
} from "./types";

type AdvancedQuizProps = {
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

function isNumericQuestion(question: QuizQuestion): boolean {
  return question.input_type === "numeric" || question.value_spec != null || question.options.length === 0;
}

function ratingDelta(submission: AdvancedSubmission): number {
  return submission.predictions.reduce((total, prediction) => total + prediction.rating_delta, 0);
}

function formatDelta(value: number): string {
  return value > 0 ? `+${value}` : `${value}`;
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "The quiz could not be submitted. Please try again.";
}

function AdvancedResult({ submission, onRestart, onChooseDifferentMode }: {
  submission: AdvancedSubmission;
  onRestart: () => void;
  onChooseDifferentMode: () => void;
}) {
  const pending = submission.attempt.status === "pending";

  return (
    <Card as="section" className={`quiz-result${pending ? " quiz-pending" : ""}`} data-testid="quiz-result" aria-live="polite">
      <h3>{pending ? "Advanced settlement is pending" : "Advanced Quiz results"}</h3>
      {pending ? (
        <>
          <p>Your answers were accepted. The server is waiting for authoritative settlement data.</p>
          <p>We will not estimate a score or rating change while settlement is pending.</p>
          <p className="quiz-help">Attempt: {submission.attempt.id}</p>
        </>
      ) : (
        <>
          <p><strong>Server score:</strong> {submission.attempt.score}%</p>
          <p><strong>Correct answers:</strong> {submission.attempt.correct_answers} of {submission.attempt.total_questions}</p>
          <p><strong>Rating change:</strong> {formatDelta(ratingDelta(submission))}</p>
          <p><strong>Server rating:</strong> {submission.rating.rating}</p>
        </>
      )}
      <div className="quiz-actions">
        <Button className="quiz-action" onClick={onRestart}>Start another Advanced Quiz</Button>
        <Button className="quiz-action" variant="secondary" onClick={onChooseDifferentMode}>
          Choose another mode
        </Button>
      </div>
    </Card>
  );
}

export function AdvancedQuiz({ questions, onChooseDifferentMode }: AdvancedQuizProps) {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [startedAt, setStartedAt] = useState(() => new Date().toISOString());
  const [validationError, setValidationError] = useState<string | null>(null);
  const [submission, setSubmission] = useState<AdvancedSubmission | null>(null);
  const submitQuiz = useSubmitQuiz();
  const submittingRef = useRef(false);
  const attemptIdRef = useRef(createAttemptId());
  const payloadRef = useRef<AdvancedSubmitRequest | null>(null);

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

  function validateAnswers(): string | null {
    for (const question of questions) {
      const answer = answers[question.id]?.trim();
      if (!answer) return "Answer every question before submitting.";

      if (isNumericQuestion(question)) {
        const numericAnswer = Number(answer);
        if (!Number.isFinite(numericAnswer)) {
          return "Advanced predictions must be valid numbers.";
        }
        const minimum = question.value_spec?.min == null ? undefined : Number(question.value_spec.min);
        const maximum = question.value_spec?.max == null ? undefined : Number(question.value_spec.max);
        if (minimum !== undefined && Number.isFinite(minimum) && numericAnswer < minimum) {
          return `Prediction for “${question.question_text}” is below the allowed minimum.`;
        }
        if (maximum !== undefined && Number.isFinite(maximum) && numericAnswer > maximum) {
          return `Prediction for “${question.question_text}” is above the allowed maximum.`;
        }
      }
    }
    return null;
  }

  async function handleSubmit() {
    if (submittingRef.current || submitQuiz.isPending || submission) return;

    const error = validateAnswers();
    if (error) {
      setValidationError(error);
      const firstUnanswered = questions.find((question) => !answers[question.id]?.trim());
      if (firstUnanswered) document.getElementById(`question-${firstUnanswered.id}`)?.focus();
      return;
    }

    const payload = payloadRef.current ?? {
      attempt_id: attemptIdRef.current,
      predictions: questions.map((question) => {
        const answer = answers[question.id].trim();
        return isNumericQuestion(question)
          ? { question_id: question.id, value: answer }
          : { question_id: question.id, option_id: answer };
      }),
      started_at: startedAt,
      completed_at: new Date().toISOString(),
    };
    payloadRef.current = payload;
    submittingRef.current = true;
    setValidationError(null);
    try {
      const result = await submitQuiz.mutateAsync({
        mode: "advanced",
        payload,
      });
      if ("predictions" in result) {
        setSubmission(result);
      } else {
        setValidationError("The server returned an unexpected Advanced Quiz result.");
        resetSubmissionAttempt();
      }
    } catch (submitError) {
      setValidationError(getErrorMessage(submitError));
      if (!isApiClientError(submitError) || !submitError.isRetryable) resetSubmissionAttempt();
    } finally {
      submittingRef.current = false;
    }
  }

  const retryLocked = Boolean(payloadRef.current && validationError);

  if (submission) {
    return (
      <AdvancedResult
        submission={submission}
        onRestart={restart}
        onChooseDifferentMode={onChooseDifferentMode}
      />
    );
  }

  return (
    <Card as="section" className="quiz-panel" aria-labelledby="advanced-quiz-heading">
      <div className="quiz-panel-header">
        <div>
          <h2 id="advanced-quiz-heading">Advanced Quiz</h2>
          <p>{questions.length} prediction question{questions.length === 1 ? "" : "s"}.</p>
        </div>
        <span className="quiz-status">The server owns settlement, scoring, and rating.</span>
      </div>

      <div className="quiz-question-list">
        {questions.map((question, index) => {
          const numeric = isNumericQuestion(question);
          const spec = question.value_spec;
          return (
            <fieldset className="quiz-question" key={question.id}>
              <legend id={`question-${question.id}`} tabIndex={-1}>
                <span className="quiz-category">Prediction {index + 1} · {question.category}</span>
                {question.question_text}
              </legend>
              {numeric ? (
                <>
                  <NumberField
                    id={`prediction-${question.id}`}
                    className="quiz-number-input"
                    label={`Enter your prediction${spec?.unit_code ? ` in ${spec.unit_code}` : ""}`}
                    description="The value is validated here for usability, then validated again by the server."
                    inputMode="decimal"
                    step={spec?.step ?? "any"}
                    min={spec?.min ?? undefined}
                    max={spec?.max ?? undefined}
                    value={answers[question.id] ?? ""}
                    disabled={submitQuiz.isPending || retryLocked}
                    onChange={(event) => {
                      setAnswers((current) => ({ ...current, [question.id]: event.target.value }));
                      setValidationError(null);
                    }}
                  />
                </>
              ) : (
                <div className="quiz-options">
                  {question.options
                    .slice()
                    .sort((left, right) => left.position - right.position)
                    .map((option) => (
                      <label className="quiz-option" key={option.id}>
                        <input
                          type="radio"
                          name={`prediction-${question.id}`}
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
              )}
            </fieldset>
          );
        })}
      </div>

      {validationError ? <div className="quiz-error" role="alert">{validationError}</div> : null}
      <div className="quiz-actions">
        <Button
          className="quiz-action"
          data-testid="quiz-submit"
          disabled={submitQuiz.isPending}
          isLoading={submitQuiz.isPending}
          loadingLabel="Submitting Advanced Quiz"
          aria-busy={submitQuiz.isPending}
          onClick={() => void handleSubmit()}
        >
          {payloadRef.current && validationError ? "Retry submission" : "Submit Advanced Quiz"}
        </Button>
        <Button className="quiz-action" variant="secondary" onClick={onChooseDifferentMode}>
          Back to modes
        </Button>
      </div>
    </Card>
  );
}
