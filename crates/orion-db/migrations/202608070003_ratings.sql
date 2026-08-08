-- Ratings are stored as integer Elo values. The migration is safe to run in
-- installations where the extensions migration has not yet enabled pgcrypto.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE user_ratings (
    user_id UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    rating INTEGER NOT NULL DEFAULT 1200 CHECK (rating BETWEEN 1 AND 4000),
    games_played INTEGER NOT NULL DEFAULT 0 CHECK (games_played >= 0),
    wins INTEGER NOT NULL DEFAULT 0 CHECK (wins >= 0),
    losses INTEGER NOT NULL DEFAULT 0 CHECK (losses >= 0),
    draws INTEGER NOT NULL DEFAULT 0 CHECK (draws >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (wins + losses + draws = games_played)
);

-- This table intentionally has no question foreign key yet: questions are
-- created by the immediately following quiz migration.
CREATE TABLE question_ratings (
    question_id UUID PRIMARY KEY,
    rating INTEGER NOT NULL DEFAULT 1200 CHECK (rating BETWEEN 1 AND 4000),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    correct_answers INTEGER NOT NULL DEFAULT 0 CHECK (correct_answers >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (correct_answers <= attempts)
);

-- One immutable event is written for every user/question answer. The attempt
-- foreign key is added in 202608070004_quiz.sql after quiz_attempts exists.
CREATE TABLE rating_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attempt_id UUID,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    question_id UUID NOT NULL,
    source_type TEXT NOT NULL DEFAULT 'quiz_attempt'
        CHECK (length(btrim(source_type)) > 0),
    source_id UUID NOT NULL DEFAULT gen_random_uuid(),
    quiz_type TEXT NOT NULL CHECK (quiz_type IN ('basic', 'advanced')),
    outcome SMALLINT NOT NULL CHECK (outcome IN (0, 1)),
    correct BOOLEAN NOT NULL,
    zone TEXT NOT NULL CHECK (length(btrim(zone)) > 0),
    error_pct DOUBLE PRECISION NOT NULL CHECK (error_pct BETWEEN 0 AND 100),
    k INTEGER NOT NULL CHECK (k > 0),
    sa DOUBLE PRECISION NOT NULL CHECK (sa BETWEEN 0 AND 1),
    point_delta INTEGER NOT NULL,
    user_rating_before INTEGER NOT NULL CHECK (user_rating_before BETWEEN 1 AND 4000),
    user_rating_after INTEGER NOT NULL CHECK (user_rating_after BETWEEN 1 AND 4000),
    player_elo_before INTEGER NOT NULL CHECK (player_elo_before BETWEEN 1 AND 4000),
    player_elo_after INTEGER NOT NULL CHECK (player_elo_after BETWEEN 1 AND 4000),
    question_rating_before INTEGER NOT NULL CHECK (question_rating_before BETWEEN 1 AND 4000),
    question_rating_after INTEGER NOT NULL CHECK (question_rating_after BETWEEN 1 AND 4000),
    question_elo_before INTEGER NOT NULL CHECK (question_elo_before BETWEEN 1 AND 4000),
    question_elo_after INTEGER NOT NULL CHECK (question_elo_after BETWEEN 1 AND 4000),
    rating_delta INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (rating_delta = user_rating_after - user_rating_before),
    CHECK (player_elo_before = user_rating_before),
    CHECK (player_elo_after = user_rating_after),
    CHECK (question_elo_before = question_rating_before),
    CHECK (question_elo_after = question_rating_after)
);

CREATE INDEX user_ratings_updated_at_idx
    ON user_ratings (updated_at DESC, user_id ASC);

CREATE INDEX question_ratings_rating_idx
    ON question_ratings (rating DESC, question_id ASC);

CREATE INDEX rating_events_user_created_idx
    ON rating_events (user_id, created_at DESC, id DESC);

CREATE INDEX rating_events_question_created_idx
    ON rating_events (question_id, created_at DESC, id DESC);

CREATE INDEX rating_events_attempt_idx
    ON rating_events (attempt_id, created_at ASC, id ASC);

CREATE INDEX rating_events_source_idx
    ON rating_events (source_type, source_id, created_at ASC);

-- A retried settlement must never create two audit events for the same
-- question in one attempt. NULL attempt IDs remain allowed for standalone
-- rating events.
CREATE UNIQUE INDEX rating_events_attempt_question_unique_idx
    ON rating_events (attempt_id, question_id)
    WHERE attempt_id IS NOT NULL;
