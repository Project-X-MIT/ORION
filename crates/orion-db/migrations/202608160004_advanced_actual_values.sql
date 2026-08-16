-- Provider ingestion writes one immutable/current authoritative handoff per
-- Advanced numeric question. The worker only reads this table; it never
-- treats a client submission or Redis value as an actual.
CREATE TABLE advanced_actual_values (
    question_id UUID PRIMARY KEY REFERENCES quiz_questions (id) ON DELETE CASCADE,
    value NUMERIC(38, 18) NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    source_id TEXT NOT NULL CHECK (length(btrim(source_id)) > 0),
    source_version TEXT NOT NULL CHECK (length(btrim(source_version)) > 0),
    is_final BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (available_at >= observed_at)
);

CREATE INDEX advanced_actual_values_ready_idx
    ON advanced_actual_values (available_at, question_id)
    WHERE is_final = TRUE;
