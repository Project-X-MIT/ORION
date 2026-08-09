CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (btrim(kind) <> ''),
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    body TEXT NOT NULL CHECK (btrim(body) <> ''),
    action_url TEXT,
    deduplication_key TEXT NOT NULL CHECK (btrim(deduplication_key) <> ''),
    read_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT notifications_user_deduplication_unique
        UNIQUE (user_id, deduplication_key),
    CONSTRAINT notifications_expiry_after_creation_ck
        CHECK (expires_at IS NULL OR expires_at > created_at)
);
