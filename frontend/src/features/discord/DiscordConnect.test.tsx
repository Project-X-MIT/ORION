// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../../app/config", () => ({
  loadAppConfig: () => ({
    apiBaseUrl: "/api/v1",
    discordInviteUrl: undefined,
    requestTimeoutMs: 15_000,
  }),
}));

import { DiscordConnect, safeDiscordInviteUrl } from "./DiscordConnect";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("safeDiscordInviteUrl", () => {
  it("accepts the configured Discord invite", () => {
    expect(safeDiscordInviteUrl("https://discord.gg/qXRjY4PPp"))
      .toBe("https://discord.gg/qXRjY4PPp");
    expect(safeDiscordInviteUrl("https://discord.com/invite/qXRjY4PPp"))
      .toBe("https://discord.com/invite/qXRjY4PPp");
  });

  it.each([
    undefined,
    "",
    "not a url",
    "http://discord.gg/qXRjY4PPp",
    "https://example.com/invite/qXRjY4PPp",
    "https://discord.com/channels/@me",
    "https://discord.com/invite/",
    "https://discord.gg/invite/qXRjY4PPp",
    "https://www.discord.com/invite/qXRjY4PPp",
    "https://discord.gg:444/qXRjY4PPp",
    "https://discord.gg.evil.example/qXRjY4PPp",
    "https://user:password@discord.gg/qXRjY4PPp",
  ])("rejects unsafe or missing invite %s", (value) => {
    expect(safeDiscordInviteUrl(value)).toBeUndefined();
  });
});

describe("DiscordConnect", () => {
  it("renders a protected link from approved configuration", () => {
    const markup = renderToStaticMarkup(
      <DiscordConnect inviteUrl="https://discord.gg/qXRjY4PPp" />,
    );

    expect(markup).toContain('href="https://discord.gg/qXRjY4PPp"');
    expect(markup).toContain('target="_blank"');
    expect(markup).toContain('rel="noopener noreferrer"');
    expect(markup).toContain('referrerPolicy="no-referrer"');
  });

  it("renders a disabled safe state when configuration is missing or invalid", () => {
    const markup = renderToStaticMarkup(<DiscordConnect inviteUrl="javascript:alert(1)" />);

    expect(markup).toContain("The Discord community link is not available right now.");
    expect(markup).not.toContain('href="javascript:alert(1)"');
  });

  it("loads the approved invite from runtime configuration when no build-time value exists", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({
      api_version: 1,
      request_id: "00000000-0000-4000-8000-000000000001",
      data: { invite_url: "https://discord.gg/qXRjY4PPp" },
    }), { headers: { "Content-Type": "application/json" } }));
    vi.stubGlobal("fetch", fetchMock);

    render(<DiscordConnect />);

    await waitFor(() => expect(
      screen.getByRole("link", { name: "Join ORION on Discord" }).getAttribute("href"),
    ).toBe("https://discord.gg/qXRjY4PPp"));
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/discord/invite",
      expect.objectContaining({ credentials: "include", method: "GET" }),
    );
  });
});
