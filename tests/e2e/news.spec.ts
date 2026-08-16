// TODO(Div): provide the Playwright runner/baseURL and register `/news` in
// the application router before enabling this suite in CI.
// TODO(CI): install frontend dependencies before running type-check, Vitest,
// and Playwright validation for this feature.
import { expect, test, type Page } from "@playwright/test";

const authenticatedUser = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "news-e2e@example.com",
  username: "news-e2e",
  display_name: "News E2E",
  status: "active",
  role: "user",
};

const article = {
  id: "00000000-0000-0000-0000-000000000011",
  source_id: "00000000-0000-0000-0000-000000000021",
  source_name: "SEC",
  source_slug: "sec",
  title: "Markets open higher",
  summary: "A concise market summary.",
  content: "Full content is not displayed in the feed card.",
  url: "https://www.sec.gov/news/story",
  image_url: null,
  author: null,
  category: "markets",
  symbols: ["ORION"],
  published_at: "2026-08-14T10:00:00Z",
};

function successBody(data: unknown) {
  return JSON.stringify({
    api_version: 1,
    request_id: "news-e2e-request",
    data,
  });
}

async function stubSession(page: Page) {
  await page.route("**/api/v1/auth/me*", (route) => route.fulfill({
    status: 200,
    contentType: "application/json",
    body: successBody({ user: authenticatedUser }),
  }));
}

test.describe("news feed", () => {
  test("renders source, UTC time, summary, and a safe original link", async ({ page }) => {
    await stubSession(page);
    await page.route("**/api/v1/news/latest*", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: successBody({
        items: [article],
        limit: 20,
        offset: 0,
        has_more: false,
        next_cursor: null,
      }),
    }));

    await page.goto("/news");

    await expect(page.getByRole("heading", { name: "Market news" })).toBeVisible();
    const card = page.getByRole("article");
    await expect(card).toContainText("SEC");
    await expect(card).toContainText("A concise market summary.");
    await expect(card.locator("time")).toHaveAttribute("dateTime", "2026-08-14T10:00:00.000Z");

    const originalLink = card.getByRole("link", { name: /Read Markets open higher from SEC/ });
    await expect(originalLink).toHaveAttribute("href", article.url);
    await expect(originalLink).toHaveAttribute("target", "_blank");
    await expect(originalLink).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("renders the empty state", async ({ page }) => {
    await stubSession(page);
    await page.route("**/api/v1/news/latest*", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: successBody({
        items: [],
        limit: 20,
        offset: 0,
        has_more: false,
        next_cursor: null,
      }),
    }));

    await page.goto("/news");

    await expect(page.getByRole("status")).toContainText("No news is available right now.");
  });

  test("supports keyboard recovery at a narrow viewport", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await stubSession(page);
    let attempts = 0;
    await page.route("**/api/v1/news/latest*", (route) => {
      attempts += 1;
      if (attempts === 1) {
        return route.fulfill({
          status: 503,
          contentType: "application/json",
          body: JSON.stringify({
            api_version: 1,
            request_id: "news-e2e-keyboard-error",
            error: { code: "SERVICE_UNAVAILABLE", message: "news unavailable" },
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: successBody({
          items: [article],
          limit: 20,
          offset: 0,
          has_more: false,
          next_cursor: null,
        }),
      });
    });

    await page.goto("/news");

    const retry = page.getByRole("button", { name: "Try again" });
    await retry.focus();
    await expect(retry).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("article")).toContainText("Markets open higher");
    await expect(page.locator("main.news-feed")).toBeVisible();
  });

  test("renders a recoverable error when the feed API fails", async ({ page }) => {
    await stubSession(page);
    await page.route("**/api/v1/news/latest*", (route) => route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({
        api_version: 1,
        request_id: "news-e2e-error",
        error: {
          code: "SERVICE_UNAVAILABLE",
          message: "news is temporarily unavailable",
        },
      }),
    }));

    await page.goto("/news");

    await expect(page.getByRole("alert")).toContainText("News is temporarily unavailable.");
    await expect(page.getByRole("status")).toContainText("No recent articles are available right now.");
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
  });
});
