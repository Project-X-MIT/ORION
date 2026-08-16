import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

type Fixture = {
  frontendUrl?: string;
  credentials: { email: string; password: string };
  userId: string;
};

function fixture(): Fixture | null {
  const raw = process.env.PROFILE_E2E_FIXTURE;
  if (!raw) return null;
  const value = JSON.parse(raw) as Fixture;
  if (!value.credentials?.email || !value.credentials.password || !value.userId) {
    throw new Error("PROFILE_E2E_FIXTURE requires credentials and userId");
  }
  return value;
}

const profileFixture = fixture();

function appUrl(path: string): string {
  const base = profileFixture?.frontendUrl ?? process.env.PROFILE_E2E_FRONTEND_URL ?? "http://127.0.0.1:5173";
  return `${base.replace(/\/$/u, "")}${path}`;
}

async function signIn(page: Page) {
  if (!profileFixture) throw new Error("PROFILE_E2E_FIXTURE was not loaded");
  await page.goto(appUrl("/login"));
  await page.getByLabel("Email").fill(profileFixture.credentials.email);
  await page.getByLabel("Password").fill(profileFixture.credentials.password);
  const loginResponse = page.waitForResponse((response) =>
    response.request().method() === "POST" && response.url().includes("/auth/login") && response.ok(),
  );
  await page.getByRole("button", { name: "Sign in" }).click();
  await loginResponse;
}

test.describe("public profile dashboard", () => {
  test.skip(!profileFixture, "Set PROFILE_E2E_FIXTURE with a seeded active user to run this suite.");

  test("renders privacy-filtered profile values and accessible chart tables", async ({ page }) => {
    await signIn(page);
    await page.goto(appUrl(`/profiles/${encodeURIComponent(profileFixture?.userId ?? "")}`));
    await expect(page.getByText("Public profile", { exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Elo history" })).toBeVisible();
    await expect(page.getByRole("table", { name: "Elo history data" })).toBeAttached();
    await expect(page.getByRole("table", { name: "Rank history data" })).toBeAttached();
    await expect(page.getByRole("table", { name: "Quiz performance data" })).toBeAttached();
    await expect(page.getByText("Published research")).toBeVisible();
    await expect(page.locator("body")).not.toContainText("password_hash");
    await expect(page.locator("body")).not.toContainText("email_verified_at");
    const axe = await new AxeBuilder({ page }).analyze();
    expect(axe.violations).toEqual([]);
  });
});
