-- Advanced settlement persists the provider-backed facts used by the shared
-- domain scorer. Basic rating events keep the nullable columns empty.
ALTER TABLE rating_events
    DROP CONSTRAINT IF EXISTS rating_events_error_pct_check,
    DROP CONSTRAINT IF EXISTS rating_events_k_check;

ALTER TABLE rating_events
    ADD CONSTRAINT rating_events_error_pct_check CHECK (error_pct >= 0),
    ADD CONSTRAINT rating_events_k_check CHECK (k >= 0),
    ADD COLUMN IF NOT EXISTS advanced_prediction_value NUMERIC(38, 18),
    ADD COLUMN IF NOT EXISTS advanced_actual_value NUMERIC(38, 18),
    ADD COLUMN IF NOT EXISTS advanced_actual_observed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS advanced_actual_available_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS advanced_actual_source_id TEXT,
    ADD COLUMN IF NOT EXISTS advanced_actual_source_version TEXT,
    ADD COLUMN IF NOT EXISTS advanced_relative_error_pct NUMERIC(38, 18),
    ADD COLUMN IF NOT EXISTS elo_policy_version INTEGER;

ALTER TABLE rating_events
    ADD CONSTRAINT rating_events_advanced_source_pair_check CHECK (
        (advanced_actual_source_id IS NULL AND advanced_actual_source_version IS NULL)
        OR (
            length(btrim(advanced_actual_source_id)) > 0
            AND length(btrim(advanced_actual_source_version)) > 0
        )
    );

CREATE INDEX IF NOT EXISTS rating_events_advanced_source_idx
    ON rating_events (advanced_actual_source_id, advanced_actual_source_version)
    WHERE advanced_actual_source_id IS NOT NULL;
