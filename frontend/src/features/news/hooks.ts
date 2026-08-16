import { useQuery } from "@tanstack/react-query";

import { getLatestNews } from "./api";
import type { NewsFeedQuery } from "./types";

export function useNewsFeed(query: NewsFeedQuery = {}) {
  return useQuery({
    queryKey: ["news", "latest", query],
    queryFn: ({ signal }) => getLatestNews(query, signal),
  });
}
