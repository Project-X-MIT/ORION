-- Immutable Advanced question facts required by the provider-backed worker.
-- Existing Basic and option-shaped Advanced questions remain compatible: the
-- contract is nullable until a numeric question is explicitly configured.
ALTER TABLE quiz_questions
    ADD COLUMN IF NOT EXISTS advanced_unit_code TEXT,
    ADD COLUMN IF NOT EXISTS advanced_currency_code TEXT,
    ADD COLUMN IF NOT EXISTS advanced_value_scale INTEGER,
    ADD COLUMN IF NOT EXISTS advanced_market_calendar_id TEXT,
    ADD COLUMN IF NOT EXISTS advanced_market_calendar_version TEXT,
    ADD COLUMN IF NOT EXISTS advanced_market_timezone TEXT,
    ADD COLUMN IF NOT EXISTS advanced_horizon_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS advanced_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS advanced_provider_key TEXT;

ALTER TABLE quiz_questions
    ADD CONSTRAINT quiz_questions_advanced_contract_ck CHECK (
        (
            advanced_unit_code IS NULL
            AND advanced_currency_code IS NULL
            AND advanced_value_scale IS NULL
            AND advanced_market_calendar_id IS NULL
            AND advanced_market_calendar_version IS NULL
            AND advanced_market_timezone IS NULL
            AND advanced_horizon_at IS NULL
            AND advanced_expires_at IS NULL
            AND advanced_provider_key IS NULL
        )
        OR (
            advanced_unit_code IS NOT NULL
            AND advanced_value_scale IS NOT NULL
            AND advanced_market_calendar_id IS NOT NULL
            AND advanced_market_calendar_version IS NOT NULL
            AND advanced_market_timezone IS NOT NULL
            AND advanced_horizon_at IS NOT NULL
            AND advanced_expires_at IS NOT NULL
            AND advanced_provider_key IS NOT NULL
            AND length(btrim(advanced_unit_code)) > 0
            AND (
                advanced_currency_code IS NULL
                OR (
                    length(btrim(advanced_currency_code)) = 3
                    AND advanced_currency_code = upper(advanced_currency_code)
                )
            )
            AND advanced_value_scale BETWEEN 0 AND 18
            AND length(btrim(advanced_market_calendar_id)) > 0
            AND length(btrim(advanced_market_calendar_version)) > 0
            AND length(btrim(advanced_market_timezone)) > 0
            AND advanced_horizon_at IS NOT NULL
            AND advanced_expires_at > advanced_horizon_at
            AND length(btrim(advanced_provider_key)) > 0
        )
    );

CREATE INDEX advanced_questions_horizon_idx
    ON quiz_questions (advanced_horizon_at, id)
    WHERE quiz_type = 'advanced' AND active = TRUE AND advanced_horizon_at IS NOT NULL;
