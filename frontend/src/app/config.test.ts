import { describe, expect, it } from "vitest";

import { createAppConfig } from "./config";

describe("application configuration", () => {
  it("provides immutable local-development defaults", () => {
    const config = createAppConfig({});

    expect(config).toEqual({
      apiBaseUrl: "/api/v1",
      discordInviteUrl: undefined,
      requestTimeoutMs: 15_000,
    });
    expect(Object.isFrozen(config)).toBe(true);
  });

  it("normalizes configured values", () => {
    expect(createAppConfig({
      VITE_API_BASE_URL: " https://api.orion.example/api/v1/// ",
      VITE_DISCORD_INVITE_URL: " https://discord.gg/qXRjY4PPp ",
      VITE_API_REQUEST_TIMEOUT_MS: "2500",
    })).toEqual({
      apiBaseUrl: "https://api.orion.example/api/v1",
      discordInviteUrl: "https://discord.gg/qXRjY4PPp",
      requestTimeoutMs: 2_500,
    });
  });

  it.each([
    [{ VITE_API_BASE_URL: "api/v1" }, "VITE_API_BASE_URL"],
    [{ VITE_API_REQUEST_TIMEOUT_MS: "0" }, "VITE_API_REQUEST_TIMEOUT_MS"],
    [{ VITE_API_REQUEST_TIMEOUT_MS: "1.5" }, "VITE_API_REQUEST_TIMEOUT_MS"],
  ])("rejects invalid environment values", (environment, message) => {
    expect(() => createAppConfig(environment)).toThrow(message);
  });
});
