import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { getLeaderboard } from "./api";
import {
  DEFAULT_LEADERBOARD_PAGE_SIZE,
  type LeaderboardResponse,
} from "./types";

type UseLeaderboardOptions = Readonly<{
  pageSize?: number;
}>;

export function useLeaderboard({
  pageSize = DEFAULT_LEADERBOARD_PAGE_SIZE,
}: UseLeaderboardOptions = {}) {
  const [page, setPage] = useState(1);
  // The first cursor is intentionally null. Every later cursor is kept opaque
  // and is supplied exactly as returned by the authoritative API.
  const [pageCursors, setPageCursors] = useState<readonly (string | null)[]>([null]);
  const cursor = pageCursors[page - 1] ?? null;

  const query = useQuery<LeaderboardResponse>({
    queryKey: ["leaderboard", { cursor, limit: pageSize }],
    queryFn: ({ signal }) => getLeaderboard({
      cursor: cursor ?? undefined,
      limit: pageSize,
    }, signal),
    placeholderData: keepPreviousData,
  });

  const goToNextPage = useCallback(() => {
    const nextCursor = query.data?.next_cursor;
    if (query.isFetching || !nextCursor) return;

    setPageCursors((current) => [...current.slice(0, page), nextCursor]);
    setPage((current) => current + 1);
  }, [page, query.data?.next_cursor, query.isFetching]);

  const goToPreviousPage = useCallback(() => {
    if (query.isFetching) return;
    setPage((current) => Math.max(1, current - 1));
  }, [query.isFetching]);

  return {
    ...query,
    goToNextPage,
    goToPreviousPage,
    hasNextPage: Boolean(query.data?.next_cursor),
    hasPreviousPage: page > 1,
    isChangingPage: query.isFetching && query.isPlaceholderData,
    page,
  };
}
