import { apiClient } from "../../shared/api/client";

import type { Profile } from "./types";

export const PROFILE_HISTORY_LIMIT = 100;

export function getProfile(userId: string, signal?: AbortSignal): Promise<Profile> {
  return apiClient.get<Profile>(
    `/profiles/${encodeURIComponent(userId)}?limit=${PROFILE_HISTORY_LIMIT}`,
    { signal },
  );
}
