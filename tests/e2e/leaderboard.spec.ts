import { expect, test } from "@playwright/test";

const requestId = "00000000-0000-4000-8000-000000000099";
const user = {
  id: "00000000-0000-4000-8000-000000000050",
  email: "leaderboard@example.test",
  username: "current-user",
  display_name: "Current User",
  status: "active",
  role: "user",
};

function success<T>(data: T) {
  return {
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ api_version: 1, request_id: requestId, data }),
  };
}

function failure(code: string, message: string, status: number) {
  return {
    status,
    contentType: "application/json",
    body: JSON.stringify({
      api_version: 1,
      request_id: requestId,
      error: { code, message },
    }),
  };
}

const firstPage = {
  entries: [
    {
      rank: 1,
      user_id: "00000000-0000-4000-8000-000000000001",
      username: "orion",
      display_name: "Orion",
      avatar_url: null,
      rating: 2400,
      rank_movement: null,
    },
    {
      rank: 2,
      user_id: user.id,
      username: user.username,
      display_name: user.display_name,
      avatar_url: null,
      rating: 2200,
      rank_movement: 2,
    },
  ],
  next_cursor: "page-two",
  as_of: "2026-08-17T08:30:00Z",
};

const secondPage = {
  entries: [
    {
      rank: 3,
      user_id: "00000000-0000-4000-8000-000000000003",
      username: "researcher",
      display_name: "Researcher",
      avatar_url: null,
      rating: 2100,
      rank_movement: -1,
    },
    {
      rank: 4,
      user_id: "00000000-0000-4000-8000-000000000004",
      username: "analyst",
      display_name: "Analyst",
      avatar_url: null,
      rating: 2050,
      rank_movement: 0,
    },
  ],
  next_cursor: null,
  as_of: "2026-08-17T08:30:00Z",
};

test.describe("global leaderboard", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/v1/auth/me", (route) => route.fulfill(success({ user })));
    await page.route("**/api/v1/leaderboard**", (route) => route.fulfill(success(firstPage)));
    // The feature route is registered by the application router owner.
    await page.goto("/leaderboard");
    await expect(page.getByRole("heading", { name: "Global leaderboard" })).toBeVisible();
  });

  test("renders authoritative rank, Elo, movement, and current-user context", async ({ page }) => {
    const currentUserRow = page.getByRole("row").filter({ hasText: user.username });

    await expect(currentUserRow).toContainText("You");
    await expect(currentUserRow).toContainText("2200");
    await expect(currentUserRow).toContainText("↑ 2");
    await expect(page.getByRole("region", { name: "Your leaderboard position" })).toContainText("#2");
    await expect(page.getByRole("row").filter({ hasText: "orion" })).toContainText("—");
  });

  test("keeps pagination keyboard operable and advances using the opaque cursor", async ({ page }) => {
    let requestedCursor: string | null = null;
    await page.unroute("**/api/v1/leaderboard**");
    await page.route("**/api/v1/leaderboard**", async (route) => {
      requestedCursor = new URL(route.request().url()).searchParams.get("cursor");
      await route.fulfill(success(requestedCursor === "page-two" ? secondPage : firstPage));
    });

    const pagination = page.getByRole("navigation", { name: "Leaderboard pages" });
    const before = await pagination.boundingBox();
    const next = page.getByRole("button", { name: "Next page" });
    await next.focus();
    await page.keyboard.press("Enter");

    await expect(page.getByText("Page 2", { exact: true })).toBeVisible();
    await expect(page.getByRole("row").filter({ hasText: "researcher" })).toContainText("↓ 1");
    expect(requestedCursor).toBe("page-two");
    expect(await pagination.boundingBox()).toEqual(before);
  });

  test("announces the loading state while a page is pending", async ({ page }) => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => { release = resolve; });
    await page.unroute("**/api/v1/leaderboard**");
    await page.route("**/api/v1/leaderboard**", async (route) => {
      await pending;
      await route.fulfill(success(firstPage));
    });
    await page.reload();

    await expect(page.getByRole("region", { name: "Global leaderboard" })).toHaveAttribute("aria-busy", "true");
    release();
    await expect(page.getByRole("row").filter({ hasText: user.username })).toBeVisible();
  });

  test("shows an empty state", async ({ page }) => {
    await page.unroute("**/api/v1/leaderboard**");
    await page.route("**/api/v1/leaderboard**", (route) => route.fulfill(success({
      entries: [],
      next_cursor: null,
      as_of: "2026-08-17T08:30:00Z",
    })));
    await page.getByRole("button", { name: "Refresh leaderboard" }).click();

    await expect(page.getByRole("cell", { name: "No ranked players are available yet." })).toBeVisible();
  });

  test("retains rows and exposes a recoverable stale state after refresh failure", async ({ page }) => {
    await page.unroute("**/api/v1/leaderboard**");
    await page.route("**/api/v1/leaderboard**", (route) => route.fulfill(
      failure("SERVICE_UNAVAILABLE", "Leaderboard is temporarily unavailable", 503),
    ));

    await page.getByRole("button", { name: "Refresh leaderboard" }).click();
    await expect(page.getByRole("alert")).toContainText("could not refresh");
    await expect(page.getByRole("row").filter({ hasText: user.username })).toBeVisible();
  });

  test("recovers from an initial loading failure", async ({ page }) => {
    await page.unroute("**/api/v1/leaderboard**");
    let allowRecovery = false;
    let requests = 0;
    await page.route("**/api/v1/leaderboard**", async (route) => {
      requests += 1;
      await route.fulfill(allowRecovery
        ? success(firstPage)
        : failure("SERVICE_UNAVAILABLE", "Leaderboard is temporarily unavailable", 503));
    });
    await page.reload();
    await expect(page.getByRole("alert")).toContainText("temporarily unavailable");
    allowRecovery = true;
    await page.getByRole("button", { name: "Try again" }).click();

    await expect(page.getByRole("row").filter({ hasText: user.username })).toBeVisible();
    expect(requests).toBeGreaterThanOrEqual(4);
  });

  for (const viewport of [360, 1440]) {
    test(`does not clip the page at ${viewport}px`, async ({ page }) => {
      await page.setViewportSize({ width: viewport, height: 900 });
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
      expect(overflow).toBe(false);
      await expect(page.getByRole("columnheader", { name: "Movement" })).toBeVisible();
    });
  }
});
