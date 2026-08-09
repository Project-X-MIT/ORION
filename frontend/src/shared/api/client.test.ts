import { describe, expect, it, vi } from "vitest";

import { createApiClient } from "./client";
import { ApiClientError } from "./errors";

describe("createApiClient", () => {
  it("sends cookie credentials and unwraps a versioned success response", async () => {
    const requestFetch = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({
      api_version: 1,
      request_id: "request-1",
      data: { user: { id: "user-1" } },
    }), { status: 200 }));
    const client = createApiClient({
      baseUrl: "/api/v1/",
      fetch: requestFetch as typeof fetch,
    });

    await expect(client.post("/auth/login", { email: "user@example.com" }))
      .resolves.toEqual({ user: { id: "user-1" } });

    expect(requestFetch).toHaveBeenCalledOnce();
    const [url, init] = requestFetch.mock.calls[0];
    expect(url).toBe("/api/v1/auth/login");
    expect(init).toMatchObject({ method: "POST", credentials: "include" });
    expect((init?.headers as Headers).get("Content-Type")).toBe("application/json");
    expect(init?.body).toBe(JSON.stringify({ email: "user@example.com" }));
  });

  it("preserves structured API errors and reports an expired session", async () => {
    const onUnauthenticated = vi.fn();
    const requestFetch = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => new Response(JSON.stringify({
      api_version: 1,
      request_id: "request-2",
      error: {
        code: "UNAUTHENTICATED",
        message: "authentication is required",
        details: { session: "expired" },
      },
    }), { status: 401 }));
    const client = createApiClient({
      baseUrl: "/api/v1",
      fetch: requestFetch as typeof fetch,
      onUnauthenticated,
    });

    const error = await client.get("auth/me").catch((cause: unknown) => cause);

    expect(error).toBeInstanceOf(ApiClientError);
    expect(error).toMatchObject({
      kind: "http",
      status: 401,
      code: "UNAUTHENTICATED",
      requestId: "request-2",
      details: { session: "expired" },
      isUnauthenticated: true,
    });
    expect(onUnauthenticated).toHaveBeenCalledWith(error);
  });

  it("distinguishes caller cancellation from a retryable timeout", async () => {
    const hangingFetch = vi.fn((_input: RequestInfo | URL, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        const signal = init?.signal;
        if (signal?.aborted) {
          reject(signal.reason);
          return;
        }
        signal?.addEventListener("abort", () => reject(signal.reason), { once: true });
      }));
    const client = createApiClient({
      baseUrl: "/api/v1",
      fetch: hangingFetch as typeof fetch,
      timeoutMs: 5,
    });
    const caller = new AbortController();
    caller.abort();

    const cancelled = await client.get("/cancelled", { signal: caller.signal })
      .catch((cause: unknown) => cause);
    const timedOut = await client.get("/timeout").catch((cause: unknown) => cause);

    expect(cancelled).toMatchObject({ kind: "cancelled", status: 0, isRetryable: false });
    expect(timedOut).toMatchObject({ kind: "timeout", status: 0, isRetryable: true });
  });

  it("normalizes transport failures into shared retryable errors", async () => {
    const client = createApiClient({
      baseUrl: "/api/v1",
      fetch: vi.fn(async () => { throw new TypeError("offline"); }) as typeof fetch,
    });

    await expect(client.get("/health")).rejects.toMatchObject({
      kind: "network",
      status: 0,
      isRetryable: true,
    });
  });

  it("discards a late success after cancellation or timeout", async () => {
    const lateSuccess = () => new Promise<Response>((resolve) => {
      setTimeout(() => resolve(new Response(JSON.stringify({
        api_version: 1,
        request_id: "late-request",
        data: { stale: true },
      }), { status: 200 })), 10);
    });
    const client = createApiClient({
      baseUrl: "/api/v1",
      fetch: vi.fn(lateSuccess) as typeof fetch,
      timeoutMs: 2,
    });
    const caller = new AbortController();
    caller.abort();

    await expect(client.get("/cancelled", { signal: caller.signal }))
      .rejects.toMatchObject({ kind: "cancelled" });
    await expect(client.get("/timeout"))
      .rejects.toMatchObject({ kind: "timeout" });
  });
});
