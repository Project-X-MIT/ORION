import { apiClient } from "../../shared/api/client";

import type { NewsFeedQuery, NewsFeedResponse } from "./types";

function queryString(query: NewsFeedQuery): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined && value !== "") params.set(key, String(value));
  }
  const serialized = params.toString();
  return serialized ? `?${serialized}` : "";
}

export function getLatestNews(
  query: NewsFeedQuery = {},
  signal?: AbortSignal,
): Promise<NewsFeedResponse> {
  const path = `/news/latest${queryString(query)}`;
  return signal
    ? apiClient.get<NewsFeedResponse>(path, { signal })
    : apiClient.get<NewsFeedResponse>(path);
}
