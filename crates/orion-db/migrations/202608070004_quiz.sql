CREATE TABLE quiz_questions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    quiz_type TEXT NOT NULL DEFAULT 'basic'
        CHECK (quiz_type IN ('basic', 'advanced')),
    category TEXT NOT NULL CHECK (length(btrim(category)) > 0),
    question_text TEXT NOT NULL CHECK (length(btrim(question_text)) > 0),
    explanation TEXT CHECK (explanation IS NULL OR length(btrim(explanation)) > 0),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE quiz_options (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    question_id UUID NOT NULL REFERENCES quiz_questions (id) ON DELETE CASCADE,
    option_text TEXT NOT NULL CHECK (length(btrim(option_text)) > 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    is_correct BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (question_id, position),
    UNIQUE (id, question_id)
);

CREATE TABLE quiz_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    quiz_type TEXT NOT NULL CHECK (quiz_type IN ('basic', 'advanced')),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'completed')),
    total_questions INTEGER NOT NULL CHECK (total_questions > 0),
    correct_answers INTEGER NOT NULL DEFAULT 0 CHECK (correct_answers >= 0),
    score INTEGER NOT NULL DEFAULT 0 CHECK (score BETWEEN 0 AND 100),
    rating_before INTEGER NOT NULL CHECK (rating_before BETWEEN 1 AND 4000),
    rating_after INTEGER NOT NULL CHECK (rating_after BETWEEN 1 AND 4000),
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (correct_answers <= total_questions),
    CHECK (
        (status = 'pending' AND completed_at IS NULL)
        OR (status = 'completed' AND completed_at IS NOT NULL)
    ),
    CHECK (
        status = 'pending'
        OR score = (correct_answers * 100 / total_questions)
    )
);

ALTER TABLE question_ratings
    ADD CONSTRAINT question_ratings_question_fk
    FOREIGN KEY (question_id) REFERENCES quiz_questions (id) ON DELETE CASCADE;

ALTER TABLE rating_events
    ADD CONSTRAINT rating_events_question_fk
    FOREIGN KEY (question_id) REFERENCES quiz_questions (id) ON DELETE CASCADE;

ALTER TABLE rating_events
    ADD CONSTRAINT rating_events_attempt_fk
    FOREIGN KEY (attempt_id) REFERENCES quiz_attempts (id) ON DELETE SET NULL;

CREATE INDEX quiz_questions_type_active_idx
    ON quiz_questions (quiz_type, active, id);

CREATE INDEX quiz_options_question_position_idx
    ON quiz_options (question_id, position);

-- Quiz submissions select one option per question, so a question can have at
-- most one correct option. The application still validates that one exists.
CREATE UNIQUE INDEX quiz_options_one_correct_idx
    ON quiz_options (question_id)
    WHERE is_correct = TRUE;

CREATE INDEX quiz_attempts_user_created_idx
    ON quiz_attempts (user_id, created_at DESC, id DESC);

CREATE INDEX quiz_attempts_user_completed_idx
    ON quiz_attempts (user_id, completed_at DESC)
    WHERE status = 'completed';
