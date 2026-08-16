import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiClient } from "../../shared/api/client";
import { getLatestNews } from "./api";

vi.mock("../../shared/api/client", () => ({
  apiClient: {
    get: vi.fn(),
  },
}));

const mockedGet = vi.mocked(apiClient.get);

describe("news API", () => {
  beforeEach(() => {
    mockedGet.mockReset();
    mockedGet.mockResolvedValue({
      items: [],
      limit: 20,
      offset: 0,
      has_more: false,
      next_cursor: null,
    });
  });

  it("requests the latest feed without inventing an API envelope", async () => {
    await getLatestNews();

    expect(mockedGet).toHaveBeenCalledWith("/news/latest");
  });

  it("serializes the approved filters and cursor", async () => {
    await getLatestNews({
      limit: 10,
      cursor: "opaque cursor",
      category: "global markets",
      symbol: "AAPL/B",
      source_id: "source-1",
    });

    expect(mockedGet).toHaveBeenCalledWith(
      "/news/latest?limit=10&cursor=opaque+cursor&category=global+markets&symbol=AAPL%2FB&source_id=source-1",
    );
  });

  it("omits empty optional parameters", async () => {
    await getLatestNews({ category: "", symbol: undefined });

    expect(mockedGet).toHaveBeenCalledWith("/news/latest");
  });

  it("forwards cancellation to the shared API client", async () => {
    const controller = new AbortController();
    await getLatestNews({}, controller.signal);

    expect(mockedGet).toHaveBeenCalledWith("/news/latest", { signal: controller.signal });
  });
});
