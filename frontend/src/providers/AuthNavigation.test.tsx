// @vitest-environment jsdom

import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as authApi from "../features/authentication/api";
import { apiClient } from "../shared/api/client";
import { ApiClientError } from "../shared/api/errors";
import { ProtectedRoute } from "../routes/ProtectedRoute";
import { AppProviders } from "./AppProviders";
import { useAuth } from "./AuthProvider";

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
  vi.restoreAllMocks();
  window.history.replaceState({}, "", "/");
});

function AuthStateProbe() {
  const { bootstrapError, error, login, status } = useAuth();
  return (
    <>
      <output data-testid="auth-status">{status}</output>
      <output data-testid="bootstrap-error">{bootstrapError ?? "none"}</output>
      <output data-testid="action-error">{error ?? "none"}</output>
      <button type="button" onClick={() => void login({ email: "user@example.com", password: "bad" }).catch(() => undefined)}>
        Try login
      </button>
    </>
  );
}

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

describe("authentication error lifecycle", () => {
  it("keeps a failed login recoverable instead of showing the bootstrap error", async () => {
    vi.spyOn(authApi, "getCurrentUser").mockRejectedValue(
      new ApiClientError("not signed in", { kind: "http", status: 401, code: "UNAUTHENTICATED" }),
    );
    vi.spyOn(authApi, "login").mockRejectedValue(
      new ApiClientError("invalid credentials", { kind: "http", status: 401, code: "UNAUTHENTICATED" }),
    );

    render(
      <AppProviders>
        <AuthStateProbe />
      </AppProviders>,
    );

    await waitFor(() => expect(screen.getByTestId("auth-status").textContent).toBe("signed_out"));
    expect(screen.getByTestId("bootstrap-error").textContent).toBe("none");

    await act(async () => {
      screen.getByRole("button", { name: "Try login" }).click();
    });

    expect(screen.getByTestId("action-error").textContent).toBe("invalid credentials");
    expect(screen.getByTestId("bootstrap-error").textContent).toBe("none");
  });
});
