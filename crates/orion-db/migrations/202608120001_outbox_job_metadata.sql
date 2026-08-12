-- PHANTOM-04 keeps transport state and worker execution state separate.
-- Existing outbox status values remain unchanged for compatibility; the job
-- columns make queued/running/completed/retry/dead-letter observable, with
-- bounded retry scheduling and diagnostic dead-letter context.
ALTER TABLE outbox_events
    ADD COLUMN job_status TEXT NOT NULL DEFAULT 'queued'
        CHECK (job_status IN ('queued', 'running', 'completed', 'retry', 'dead_letter')),
    ADD COLUMN job_attempts INTEGER NOT NULL DEFAULT 0
        CHECK (job_attempts >= 0),
    ADD COLUMN job_error TEXT,
    ADD COLUMN job_last_failed_at TIMESTAMPTZ,
    ADD COLUMN job_next_retry_at TIMESTAMPTZ,
    ADD COLUMN job_started_at TIMESTAMPTZ,
    ADD COLUMN job_completed_at TIMESTAMPTZ,
    ADD COLUMN job_dead_lettered_at TIMESTAMPTZ,
    ADD COLUMN job_dead_letter_reason TEXT,
    ADD COLUMN job_updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP;

CREATE INDEX outbox_events_job_status_updated_idx
    ON outbox_events (job_status, job_updated_at, id);
