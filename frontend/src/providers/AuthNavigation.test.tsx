// @vitest-environment jsdom

import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { apiClient } from "../shared/api/client";
import { ProtectedRoute } from "../routes/ProtectedRoute";
import { AppProviders } from "./AppProviders";

const authenticatedUser = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "user@example.com",
  username: "orion-user",
  display_name: "ORION User",
  status: "active",
  role: "user",
};

function apiResponse(body: object, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
  window.history.replaceState({}, "", "/");
});

describe("unauthorized navigation", () => {
  it("clears authenticated state and redirects to login with a safe return target", async () => {
    window.history.replaceState({}, "", "/research/report-1?tab=review#notes");
    const requestFetch = vi.fn()
      .mockResolvedValueOnce(apiResponse({
        api_version: 1,
        request_id: "session-request",
        data: { user: authenticatedUser },
      }))
      .mockResolvedValueOnce(apiResponse({
        api_version: 1,
        request_id: "expired-request",
        error: {
          code: "UNAUTHENTICATED",
          message: "authentication is required",
        },
      }, 401));
    vi.stubGlobal("fetch", requestFetch);

    render(
      <AppProviders>
        <ProtectedRoute><p>Protected content</p></ProtectedRoute>
      </AppProviders>,
    );
    expect(await screen.findByText("Protected content")).toBeTruthy();

    await act(async () => {
      await apiClient.get("/notifications").catch(() => undefined);
    });

    await waitFor(() => expect(window.location.pathname).toBe("/login"));
    expect(new URLSearchParams(window.location.search).get("returnTo"))
      .toBe("/research/report-1?tab=review#notes");
    expect(screen.queryByText("Protected content")).toBeNull();
  });
});
