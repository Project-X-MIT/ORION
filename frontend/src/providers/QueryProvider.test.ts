import { describe, expect, it } from "vitest";

import { ApiClientError } from "../shared/api/errors";
import { queryClient, shouldRetryQuery } from "./QueryProvider";

describe("QueryProvider defaults", () => {
  it("uses bounded retries only for shared retryable errors", () => {
    const networkError = new ApiClientError("offline", { kind: "network", status: 0 });
    const authError = new ApiClientError("sign in", {
      kind: "http",
      status: 401,
      code: "UNAUTHENTICATED",
    });
    const cancelled = new ApiClientError("cancelled", { kind: "cancelled", status: 0 });

    expect(shouldRetryQuery(0, networkError)).toBe(true);
    expect(shouldRetryQuery(1, networkError)).toBe(true);
    expect(shouldRetryQuery(2, networkError)).toBe(false);
    expect(shouldRetryQuery(0, authError)).toBe(false);
    expect(shouldRetryQuery(0, cancelled)).toBe(false);
    expect(shouldRetryQuery(0, new Error("unknown"))).toBe(false);
  });

  it("sets shared freshness, collection, refetch, and mutation behavior", () => {
    const defaults = queryClient.getDefaultOptions();

    expect(defaults.queries).toMatchObject({
      staleTime: 30_000,
      gcTime: 300_000,
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
    });
    expect(defaults.mutations?.retry).toBe(false);
  });
});
