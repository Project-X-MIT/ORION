/**
 * Contract-derived quiz types.
 *
 * These mirror docs/api/quiz.openapi.yaml. Numeric Advanced predictions are
 * exact decimal strings and are returned as pending until worker settlement.
 */
export type QuizMode = "basic" | "advanced";

export type QuizOption = {
  id: string;
  option_text: string;
  position: number;
};

export type AdvancedValueSpec = {
  unit_code: string;
  currency_code?: string | null;
  scale: number;
  min?: string | null;
  max?: string | null;
  step?: string | null;
};

export type QuizQuestion = {
  id: string;
  category: string;
  question_text: string;
  options: QuizOption[];
  input_type?: "mcq" | "numeric";
  value_spec?: AdvancedValueSpec | null;
};

export type QuestionPage = {
  items: QuizQuestion[];
  limit: number;
  offset: number;
  has_more: boolean;
};

export type BasicAnswerRequest = {
  question_id: string;
  option_id: string;
};

export type AdvancedPredictionRequest = {
  question_id: string;
  option_id?: string;
  value?: string;
};

export type BasicSubmitRequest = {
  attempt_id: string;
  answers: BasicAnswerRequest[];
  started_at?: string;
  completed_at?: string;
};

export type AdvancedSubmitRequest = {
  attempt_id: string;
  predictions: AdvancedPredictionRequest[];
  started_at?: string;
  completed_at?: string;
};

export type AttemptStatus = "pending" | "completed";

export type AttemptSummary = {
  id: string;
  quiz_type?: QuizMode;
  status: AttemptStatus;
  total_questions: number;
  correct_answers: number;
  score: number;
  rating_before: number;
  rating_after: number;
  started_at: string;
  completed_at: string | null;
};

export type Rating = {
  rating: number;
  games_played: number;
  wins: number;
  losses: number;
  draws: number;
};

export type AnswerResult = {
  question_id: string;
  correct: boolean;
  rating_delta: number;
};

export type BasicSubmission = {
  attempt: AttemptSummary;
  rating: Rating;
  answers: AnswerResult[];
};

export type AdvancedSubmission = {
  attempt: AttemptSummary;
  rating: Rating;
  predictions: AnswerResult[];
};

export type AttemptResult = {
  attempt: AttemptSummary & { quiz_type: QuizMode };
  rating: Rating;
  answers?: AnswerResult[];
  predictions?: AnswerResult[];
};

export type QuizSubmission = BasicSubmission | AdvancedSubmission;

export type QuizSubmissionInput =
  | { mode: "basic"; payload: BasicSubmitRequest }
  | { mode: "advanced"; payload: AdvancedSubmitRequest };

export function isPendingAttempt(attempt: AttemptSummary): boolean {
  return attempt.status === "pending";
}

export function isAdvancedSubmission(
  submission: QuizSubmission,
): submission is AdvancedSubmission {
  return "predictions" in submission;
}
