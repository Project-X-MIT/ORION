-- Extensions are installed before the application migration chain. The
-- application still runs its compatibility preflight and forward migrations.
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;
