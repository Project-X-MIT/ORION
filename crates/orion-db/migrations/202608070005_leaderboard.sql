CREATE TABLE leaderboard_rank_history (
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    previous_rank BIGINT CHECK (previous_rank > 0),
    current_rank BIGINT NOT NULL CHECK (current_rank > 0),
    rank_movement BIGINT GENERATED ALWAYS AS (previous_rank - current_rank) STORED,

    PRIMARY KEY (snapshot_at, user_id)
);

-- Supports a user's latest/previous rank lookups without visiting the table
-- for the projected ranking values.
CREATE INDEX leaderboard_rank_history_user_snapshot_idx
    ON leaderboard_rank_history (user_id, snapshot_at DESC)
    INCLUDE (previous_rank, current_rank, rank_movement);

-- Supports reading or comparing the ordered leaderboard for one snapshot.
CREATE INDEX leaderboard_rank_history_snapshot_rank_idx
    ON leaderboard_rank_history (snapshot_at DESC, current_rank ASC)
    INCLUDE (user_id, previous_rank, rank_movement);

-- Supports ranking directly from the authoritative current-Elo table.
CREATE INDEX user_ratings_leaderboard_idx
    ON user_ratings (rating DESC, user_id ASC);
