import { useQuery } from "@tanstack/react-query";

import { getProfile } from "./api";

export function useProfile(userId: string | undefined) {
  return useQuery({
    enabled: Boolean(userId),
    queryKey: ["profile", userId],
    queryFn: ({ signal }) => getProfile(userId as string, signal),
  });
}
