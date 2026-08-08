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
