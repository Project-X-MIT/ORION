import { apiClient } from "../../shared/api/client";

import type { LeaderboardQuery, LeaderboardResponse } from "./types";

function queryString(query: LeaderboardQuery): string {
  const params = new URLSearchParams();

  if (query.limit !== undefined) params.set("limit", String(query.limit));
  if (query.cursor) params.set("cursor", query.cursor);

  const serialized = params.toString();
  return serialized ? `?${serialized}` : "";
}

export function getLeaderboard(
  query: LeaderboardQuery = {},
  signal?: AbortSignal,
): Promise<LeaderboardResponse> {
  const path = `/leaderboard${queryString(query)}`;
  return signal
    ? apiClient.get<LeaderboardResponse>(path, { signal })
    : apiClient.get<LeaderboardResponse>(path);
}
