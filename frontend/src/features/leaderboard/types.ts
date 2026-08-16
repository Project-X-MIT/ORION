export const DEFAULT_LEADERBOARD_PAGE_SIZE = 20;

// Contract-derived from `orion-api`'s global leaderboard response.
export type LeaderboardEntry = Readonly<{
  rank: number;
  user_id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  rating: number;
  rank_movement: number | null;
}>;

export type LeaderboardResponse = Readonly<{
  entries: readonly LeaderboardEntry[];
  next_cursor: string | null;
  as_of: string;
}>;

export type LeaderboardQuery = Readonly<{
  limit?: number;
  cursor?: string;
}>;
