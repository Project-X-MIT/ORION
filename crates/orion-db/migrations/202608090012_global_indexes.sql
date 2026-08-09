CREATE INDEX IF NOT EXISTS users_status_created_idx
    ON users (status, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS notifications_user_created_idx
    ON notifications (user_id, created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS notifications_user_unread_idx
    ON notifications (user_id, created_at DESC, id DESC)
    WHERE read_at IS NULL;

CREATE INDEX IF NOT EXISTS notifications_expiry_idx
    ON notifications (expires_at, id)
    WHERE expires_at IS NOT NULL;
