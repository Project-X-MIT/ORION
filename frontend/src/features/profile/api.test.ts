import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiClient } from "../../shared/api/client";
import { getProfile } from "./api";

vi.mock("../../shared/api/client", () => ({ apiClient: { get: vi.fn() } }));

const mockedGet = vi.mocked(apiClient.get);

describe("profile API", () => {
  beforeEach(() => mockedGet.mockResolvedValue({}));

  it("requests the versioned public profile route with a bounded history limit", async () => {
    await getProfile("user/1");
    expect(mockedGet).toHaveBeenCalledWith("/profiles/user%2F1?limit=100", { signal: undefined });
  });

  it("forwards cancellation to the shared client", async () => {
    const controller = new AbortController();
    await getProfile("user-1", controller.signal);
    expect(mockedGet).toHaveBeenCalledWith("/profiles/user-1?limit=100", { signal: controller.signal });
  });
});
