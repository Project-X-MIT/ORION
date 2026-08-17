import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiClient } from "../../shared/api/client";
import { completeLearningLesson, getLearningProgress } from "./api";

vi.mock("../../shared/api/client", () => ({
  apiClient: {
    get: vi.fn(),
    post: vi.fn(),
  },
}));

const mockedGet = vi.mocked(apiClient.get);
const mockedPost = vi.mocked(apiClient.post);

describe("learning API", () => {
  beforeEach(() => {
    mockedGet.mockReset();
    mockedPost.mockReset();
  });

  it("reads server-owned progress with cancellation support", async () => {
    const controller = new AbortController();
    mockedGet.mockResolvedValue(undefined as never);

    await getLearningProgress(controller.signal);

    expect(mockedGet).toHaveBeenCalledWith("/learning/progress", { signal: controller.signal });
  });

  it("posts completion using an encoded lesson identity", async () => {
    mockedPost.mockResolvedValue(undefined as never);

    await completeLearningLesson("lesson/one");

    expect(mockedPost).toHaveBeenCalledWith("/learning/lessons/lesson%2Fone/completion");
  });
});
