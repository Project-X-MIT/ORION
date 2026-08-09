-- The legacy 202608070002 migration is immutable and empty. Pool startup runs
-- this idempotent migration once as a compatibility preflight before SQLx
-- applies the tracked chain, because DB-02 through DB-05 already reference
-- users. SQLx then records this migration normally after version 202608070009.
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS citext WITH SCHEMA public;

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email CITEXT NOT NULL,
    username CITEXT NOT NULL,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    bio TEXT,
    avatar_url TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    email_verified_at TIMESTAMPTZ,
    disabled_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Reconcile installations that created a minimal users table to unblock the
-- completed feature migrations before DB-01 was available.
ALTER TABLE users ADD COLUMN IF NOT EXISTS email CITEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS username CITEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS display_name TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS bio TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'active';
ALTER TABLE users ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS disabled_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE users ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP;

ALTER TABLE users ALTER COLUMN email TYPE CITEXT USING email::citext;
ALTER TABLE users ALTER COLUMN username TYPE CITEXT USING username::citext;

UPDATE users
SET username = 'legacy_' || left(replace(id::text, '-', ''), 24)
WHERE username IS NULL OR btrim(username::text) = '';

UPDATE users
SET email = 'legacy-' || id::text || '@invalid.orion.local'
WHERE email IS NULL OR btrim(email::text) = '';

UPDATE users
SET password_hash = '!legacy-user-no-password!'
WHERE password_hash IS NULL OR btrim(password_hash) = '';

UPDATE users SET status = 'active' WHERE status IS NULL;
UPDATE users SET created_at = CURRENT_TIMESTAMP WHERE created_at IS NULL;
UPDATE users SET updated_at = CURRENT_TIMESTAMP WHERE updated_at IS NULL;

ALTER TABLE users ALTER COLUMN email SET NOT NULL;
ALTER TABLE users ALTER COLUMN username SET NOT NULL;
ALTER TABLE users ALTER COLUMN password_hash SET NOT NULL;
ALTER TABLE users ALTER COLUMN status SET NOT NULL;
ALTER TABLE users ALTER COLUMN status SET DEFAULT 'active';
ALTER TABLE users ALTER COLUMN created_at SET NOT NULL;
ALTER TABLE users ALTER COLUMN created_at SET DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE users ALTER COLUMN updated_at SET NOT NULL;
ALTER TABLE users ALTER COLUMN updated_at SET DEFAULT CURRENT_TIMESTAMP;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_email_not_blank_ck'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_email_not_blank_ck
            CHECK (btrim(email::text) <> '');
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_username_format_ck'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_username_format_ck
            CHECK (username::text ~ '^[A-Za-z0-9_-]{3,32}$');
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_password_hash_not_blank_ck'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_password_hash_not_blank_ck
            CHECK (btrim(password_hash) <> '');
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_status_ck'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_status_ck
            CHECK (status IN ('active', 'disabled', 'deleted'));
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_lifecycle_ck'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_lifecycle_ck CHECK (
            (status = 'active' AND disabled_at IS NULL AND deleted_at IS NULL)
            OR (status = 'disabled' AND disabled_at IS NOT NULL AND deleted_at IS NULL)
            OR (status = 'deleted' AND deleted_at IS NOT NULL)
        );
    END IF;
END;
$$;

CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique_idx ON users (email);
CREATE UNIQUE INDEX IF NOT EXISTS users_username_unique_idx ON users (username);

CREATE OR REPLACE FUNCTION users_set_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS users_updated_at_trg ON users;
CREATE TRIGGER users_updated_at_trg
BEFORE UPDATE ON users
FOR EACH ROW
EXECUTE FUNCTION users_set_updated_at();
