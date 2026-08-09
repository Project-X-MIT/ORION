-- The shared ledger records every user Elo change, including non-quiz awards.
-- It is append-only so a correction is represented by a new compensating row.
CREATE TABLE rating_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (id),
    source_type TEXT NOT NULL CHECK (length(btrim(source_type)) > 0),
    source_id UUID NOT NULL,
    dedupe_key TEXT NOT NULL CHECK (length(btrim(dedupe_key)) > 0),
    rating_before INTEGER NOT NULL CHECK (rating_before BETWEEN 1 AND 4000),
    rating_after INTEGER NOT NULL CHECK (rating_after BETWEEN 1 AND 4000),
    rating_delta INTEGER NOT NULL CHECK (rating_delta = rating_after - rating_before),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (source_type, source_id, dedupe_key)
);

CREATE INDEX rating_ledger_user_created_idx
    ON rating_ledger (user_id, created_at DESC, id DESC);

CREATE OR REPLACE FUNCTION prevent_rating_ledger_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'rating_ledger is append-only';
END;
$$;

CREATE TRIGGER rating_ledger_append_only
BEFORE UPDATE OR DELETE ON rating_ledger
FOR EACH ROW EXECUTE FUNCTION prevent_rating_ledger_mutation();
