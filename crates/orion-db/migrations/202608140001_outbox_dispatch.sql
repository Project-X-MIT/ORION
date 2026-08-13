-- Platform-owned delivery metadata. PostgreSQL remains authoritative; Redis
-- is only a disposable hint/cache transport.
ALTER TABLE outbox_events
    ADD COLUMN IF NOT EXISTS schema_version INTEGER NOT NULL DEFAULT 1
        CHECK (schema_version > 0),
    ADD COLUMN IF NOT EXISTS request_id UUID,
    ADD COLUMN IF NOT EXISTS trace_id TEXT,
    ADD COLUMN IF NOT EXISTS lease_until TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS outbox_events_dispatch_idx
    ON outbox_events (status, job_status, created_at, id);
