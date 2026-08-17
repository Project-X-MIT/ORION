import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { DiscordConnect, safeDiscordInviteUrl } from "./DiscordConnect";

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
});
