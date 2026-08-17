-- Numeric Advanced predictions are accepted before the provider-backed
-- settlement. PostgreSQL stores the exact value; the worker owns actual-value
-- validation, scoring, Elo, and the immutable rating ledger.
CREATE TABLE advanced_predictions (
    attempt_id UUID NOT NULL REFERENCES quiz_attempts (id) ON DELETE CASCADE,
    question_id UUID NOT NULL REFERENCES quiz_questions (id) ON DELETE RESTRICT,
    value NUMERIC(38, 18) NOT NULL,
    submitted_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (attempt_id, question_id),
    CHECK (value = value)
);

CREATE INDEX advanced_predictions_question_submitted_idx
    ON advanced_predictions (question_id, submitted_at, attempt_id);

CREATE INDEX advanced_predictions_pending_attempt_idx
    ON advanced_predictions (attempt_id, question_id);
