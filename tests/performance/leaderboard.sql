\set ON_ERROR_STOP on
\timing on

-- Synthetic scale: 100,000 users and one authoritative current-Elo row each.
CREATE TEMP TABLE users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL,
    display_name TEXT,
    avatar_url TEXT
);

CREATE TEMP TABLE user_ratings (
    user_id UUID PRIMARY KEY,
    rating INTEGER NOT NULL
);

CREATE TEMP TABLE leaderboard_rank_history (
    snapshot_at TIMESTAMPTZ NOT NULL,
    user_id UUID NOT NULL,
    previous_rank BIGINT,
    current_rank BIGINT NOT NULL,
    rank_movement BIGINT GENERATED ALWAYS AS (previous_rank - current_rank) STORED,
    PRIMARY KEY (snapshot_at, user_id)
);

INSERT INTO users (id, username, display_name, avatar_url)
SELECT
    md5(user_number::text)::uuid,
    'user_' || lpad(user_number::text, 6, '0'),
    'Synthetic User ' || user_number,
    NULL
FROM generate_series(1, 100000) AS users(user_number);

INSERT INTO user_ratings (user_id, rating)
SELECT
    md5(user_number::text)::uuid,
    800 + ((user_number * 37) % 1601)
FROM generate_series(1, 100000) AS users(user_number);

CREATE INDEX user_ratings_leaderboard_idx
    ON user_ratings (rating DESC, user_id ASC);

VACUUM (ANALYZE) users;
VACUUM (ANALYZE) user_ratings;

-- Approved hourly movement window. The transaction's table lock serializes
-- overlapping writers; the timestamp primary key makes retries idempotent.
BEGIN;
LOCK TABLE leaderboard_rank_history IN SHARE ROW EXCLUSIVE MODE;
INSERT INTO leaderboard_rank_history (snapshot_at, user_id, previous_rank, current_rank)
SELECT
    TIMESTAMPTZ '2026-08-17 09:00:00+00',
    user_id,
    NULL,
    ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC)
FROM user_ratings
ON CONFLICT (snapshot_at, user_id) DO NOTHING;
COMMIT;

-- Mutate a deterministic slice, then capture the current hour against the
-- immediately preceding completed snapshot used by the API movement contract.
UPDATE user_ratings
SET rating = rating + 100
WHERE user_id IN (
    SELECT user_id FROM user_ratings ORDER BY user_id LIMIT 1000
);

BEGIN;
LOCK TABLE leaderboard_rank_history IN SHARE ROW EXCLUSIVE MODE;
INSERT INTO leaderboard_rank_history (snapshot_at, user_id, previous_rank, current_rank)
WITH ranked AS (
    SELECT user_id, ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
    FROM user_ratings
), previous AS (
    SELECT user_id, current_rank
    FROM leaderboard_rank_history
    WHERE snapshot_at = TIMESTAMPTZ '2026-08-17 09:00:00+00'
)
SELECT TIMESTAMPTZ '2026-08-17 10:00:00+00', ranked.user_id, previous.current_rank, ranked.rank
FROM ranked
LEFT JOIN previous USING (user_id)
ON CONFLICT (snapshot_at, user_id) DO NOTHING;
COMMIT;

-- Retry the same logical snapshot and prove it is a no-op.
WITH retry AS (
    INSERT INTO leaderboard_rank_history (snapshot_at, user_id, previous_rank, current_rank)
    SELECT snapshot_at, user_id, previous_rank, current_rank
    FROM leaderboard_rank_history
    WHERE snapshot_at = TIMESTAMPTZ '2026-08-17 10:00:00+00'
    ON CONFLICT (snapshot_at, user_id) DO NOTHING
    RETURNING 1
)
SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE 1 / 0 END AS retry_is_idempotent FROM retry;

DO $$
BEGIN
    IF (SELECT COUNT(*) FROM leaderboard_rank_history WHERE snapshot_at = TIMESTAMPTZ '2026-08-17 10:00:00+00') <> 100000 THEN
        RAISE EXCEPTION 'snapshot omitted or duplicated users';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM leaderboard_rank_history current
        JOIN leaderboard_rank_history previous USING (user_id)
        WHERE current.snapshot_at = TIMESTAMPTZ '2026-08-17 10:00:00+00'
          AND previous.snapshot_at = TIMESTAMPTZ '2026-08-17 09:00:00+00'
          AND current.previous_rank <> previous.current_rank
    ) THEN
        RAISE EXCEPTION 'movement does not use the approved prior snapshot';
    END IF;
END
$$;

EXPLAIN (ANALYZE, BUFFERS)
SELECT user_id, previous_rank, current_rank, rank_movement
FROM leaderboard_rank_history
WHERE snapshot_at = TIMESTAMPTZ '2026-08-17 10:00:00+00'
ORDER BY current_rank;

CREATE TEMP TABLE leaderboard_result AS
WITH ranked_users AS (
    SELECT
        user_id,
        rating,
        ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
    FROM user_ratings
)
SELECT
    ranked.rank,
    users.id AS user_id,
    users.username,
    users.display_name,
    users.avatar_url,
    ranked.rating
FROM ranked_users AS ranked
INNER JOIN users ON users.id = ranked.user_id
ORDER BY ranked.rating DESC, ranked.user_id ASC;

DO $$
BEGIN
    IF (SELECT COUNT(*) FROM leaderboard_result) <> 100000 THEN
        RAISE EXCEPTION 'leaderboard omitted or duplicated users';
    END IF;

    IF (
        SELECT COUNT(DISTINCT rank) <> 100000
            OR MIN(rank) <> 1
            OR MAX(rank) <> 100000
        FROM leaderboard_result
    ) THEN
        RAISE EXCEPTION 'rank calculation is not unique and contiguous';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM (
            SELECT
                rating,
                user_id,
                LAG(rating) OVER (ORDER BY rank) AS prior_rating,
                LAG(user_id) OVER (ORDER BY rank) AS prior_user_id
            FROM leaderboard_result
        ) AS ordered
        WHERE prior_rating < rating
           OR (prior_rating = rating AND prior_user_id > user_id)
    ) THEN
        RAISE EXCEPTION 'leaderboard ordering or tie-breaking is incorrect';
    END IF;
END
$$;

SELECT
    (SELECT COUNT(*) FROM users) AS user_count,
    (SELECT COUNT(*) FROM user_ratings) AS rating_count,
    (SELECT COUNT(*) FROM leaderboard_result) AS ranked_user_count;

-- First page.
EXPLAIN (ANALYZE, BUFFERS)
WITH ranked_users AS (
    SELECT
        user_id,
        rating,
        ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
    FROM user_ratings
)
SELECT
    ranked.rank,
    users.id AS user_id,
    users.username,
    users.display_name,
    users.avatar_url,
    ranked.rating
FROM ranked_users AS ranked
INNER JOIN users ON users.id = ranked.user_id
ORDER BY ranked.rating DESC, ranked.user_id ASC
LIMIT 100
OFFSET 0;

-- Deep page, used to expose OFFSET degradation.
EXPLAIN (ANALYZE, BUFFERS)
WITH ranked_users AS (
    SELECT
        user_id,
        rating,
        ROW_NUMBER() OVER (ORDER BY rating DESC, user_id ASC) AS rank
    FROM user_ratings
)
SELECT
    ranked.rank,
    users.id AS user_id,
    users.username,
    users.display_name,
    users.avatar_url,
    ranked.rating
FROM ranked_users AS ranked
INNER JOIN users ON users.id = ranked.user_id
ORDER BY ranked.rating DESC, ranked.user_id ASC
LIMIT 100
OFFSET 90000;
