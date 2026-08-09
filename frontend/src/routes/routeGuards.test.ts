import { describe, expect, it } from "vitest";

import { getLoginRedirect } from "./ProtectedRoute";
import { getAuthenticatedRedirect } from "./PublicRoute";

describe("route guard redirects", () => {
  it("preserves the protected location through authentication", () => {
    const redirect = getLoginRedirect({
      pathname: "/research/report-1",
      search: "?tab=analysis",
      hash: "#summary",
    });

    expect(redirect).toBe(
      "/login?returnTo=%2Fresearch%2Freport-1%3Ftab%3Danalysis%23summary",
    );
    expect(getAuthenticatedRedirect(new URL(redirect, "https://orion.local").search))
      .toBe("/research/report-1?tab=analysis#summary");
  });

  it("rejects external and authentication-loop return targets", () => {
    expect(getAuthenticatedRedirect("?returnTo=//example.com/account")).toBe("/");
    expect(getAuthenticatedRedirect("?returnTo=/login")).toBe("/");
    expect(getAuthenticatedRedirect("?returnTo=/register?invite=1")).toBe("/");
  });
});
