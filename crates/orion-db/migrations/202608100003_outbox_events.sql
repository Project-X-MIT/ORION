-- Durable event writes are committed with the business transaction that
-- produces them. A future dispatcher will transition rows out of `pending`.
CREATE TABLE outbox_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL CHECK (length(btrim(event_type)) > 0),
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'dispatched', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    dispatched_at TIMESTAMPTZ,
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0)
);

CREATE INDEX outbox_events_status_created_at_idx
    ON outbox_events (status, created_at, id);

CREATE INDEX outbox_events_created_at_idx
    ON outbox_events (created_at, id);
