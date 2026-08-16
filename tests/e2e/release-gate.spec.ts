import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const unauthenticated = {
  api_version: 1,
  request_id: "00000000-0000-0000-0000-000000000001",
  error: { code: "UNAUTHENTICATED", message: "sign in required" },
};

const user = {
  id: "00000000-0000-0000-0000-000000000002",
  username: "synthetic-release-user",
  display_name: "Synthetic Release User",
  email: "release-user@synthetic.invalid",
};

async function noHorizontalOverflow(page: Page) {
  const widths = await page.evaluate(() => ({
    documentWidth: document.documentElement.scrollWidth,
    bodyWidth: document.body.scrollWidth,
    viewportWidth: document.documentElement.clientWidth,
  }));
  expect(widths.documentWidth).toBeLessThanOrEqual(widths.viewportWidth);
  expect(widths.bodyWidth).toBeLessThanOrEqual(widths.viewportWidth);
}

async function expectAccessible(page: Page) {
  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
}

async function mockSignedOut(page: Page) {
  await page.route("**/api/v1/auth/me", (route) => route.fulfill({
    status: 401,
    contentType: "application/json",
    body: JSON.stringify(unauthenticated),
  }));
}

test.describe("release UAT matrix", () => {
  test("public authentication journeys pass accessibility and responsive checks", async ({ page }) => {
    await mockSignedOut(page);

    for (const path of ["/login", "/register"]) {
      await page.goto(path);
      await expect(page.getByRole("main")).toBeVisible();
      await noHorizontalOverflow(page);
      await expectAccessible(page);
    }
  });

  test("authenticated shell passes a signed-in smoke journey", async ({ page }) => {
    await page.route("**/api/v1/auth/me", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        api_version: 1,
        request_id: "00000000-0000-0000-0000-000000000003",
        data: { user },
      }),
    }));

    await page.goto("/");
    await expect(page.getByRole("heading", { name: /Welcome to ORION/ })).toBeVisible();
    await noHorizontalOverflow(page);
    await expectAccessible(page);
  });

  test("login reaches the authenticated shell without losing the form contract", async ({ page }) => {
    await mockSignedOut(page);
    await page.route("**/api/v1/auth/login", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        api_version: 1,
        request_id: "00000000-0000-0000-0000-000000000004",
        data: { user },
      }),
    }));

    await page.goto("/login");
    await page.getByLabel("Email").fill(user.email);
    await page.getByLabel("Password").fill("SyntheticPassword123!");
    await page.getByRole("button", { name: "Sign in" }).click();
    await expect(page).toHaveURL(/\/$/u);
    await expect(page.getByRole("heading", { name: /Welcome to ORION/ })).toBeVisible();
    await noHorizontalOverflow(page);
  });
});
