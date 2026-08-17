import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// TODO(Div): enable after `/learning` and `/learning/lessons/:lessonId` are
// mounted in the shared application router and the learning API is registered.
const learningE2eEnabled = process.env.ORION_LEARNING_E2E_ENABLED === "1";
const courseId = "00000000-0000-0000-0000-000000000001";
const viewports = [
  { name: "mobile", width: 390, height: 844 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "desktop", width: 1280, height: 900 },
] as const;

const user = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "learning-e2e@example.com",
  username: "learning-e2e",
  display_name: "Learning E2E",
  status: "active",
  role: "user",
};

const course = {
  id: courseId,
  slug: "beginner-trading",
  title: "Beginner Trading",
  description: "Learn the foundations of markets.",
  version: 1,
  modules: [{
    id: "module-1",
    slug: "foundations",
    title: "Market Foundations",
    description: "Start with the basics.",
    display_order: 1,
    lessons: [
      {
        id: "lesson-1",
        module_id: "module-1",
        slug: "what-is-a-market",
        title: "What Is a Market?",
        summary: "Understand buyers and sellers.",
        content: "A market brings buyers and sellers together.",
        lesson_order: 1,
        estimated_minutes: 8,
      },
      {
        id: "lesson-2",
        module_id: "module-1",
        slug: "reading-a-chart",
        title: "Reading a Chart",
        summary: "Read a simple chart.",
        content: "Charts describe price movement over time.",
        lesson_order: 2,
        estimated_minutes: 10,
      },
    ],
  }],
};

const progress = {
  items: [],
  summary: {
    total_modules: 1,
    completed_modules: 0,
    total_lessons: 2,
    completed_lessons: 0,
    completed: false,
  },
};

function successBody(data: unknown) {
  return JSON.stringify({
    api_version: 1,
    request_id: "learning-e2e-request",
    data,
  });
}

async function stubSession(page: Page) {
  await page.route("**/api/v1/auth/me*", (route) => route.fulfill({
    status: 200,
    contentType: "application/json",
    body: successBody({ user }),
  }));
}

async function stubLearningApi(page: Page, courseBody = course) {
  await page.route(`**/api/v1/learning/courses/${courseId}`, (route) => route.fulfill({
    status: 200,
    contentType: "application/json",
    body: successBody(courseBody),
  }));
  await page.route("**/api/v1/learning/progress", (route) => route.fulfill({
    status: 200,
    contentType: "application/json",
    body: successBody(progress),
  }));
}

async function expectNoHorizontalOverflow(page: Page) {
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

test.describe("learning experience", () => {
  test.skip(
    !learningE2eEnabled,
    "Set ORION_LEARNING_E2E_ENABLED=1 after Div mounts the learning routes and registers the API.",
  );

  for (const viewport of viewports) {
    test(`is responsive, keyboard reachable, and accessible on ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await stubSession(page);
      await stubLearningApi(page);

      await page.goto("/learning");

      await expect(page.getByRole("heading", { name: "Beginner Trading" })).toBeVisible();
      await expect(page.getByRole("link", { name: "Continue with What Is a Market?" })).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await expectAccessible(page);

      const lessonLink = page.getByRole("link", { name: /What Is a Market\?/ }).first();
      await lessonLink.focus();
      await expect(lessonLink).toBeFocused();
    });
  }

  test("supports keyboard lesson navigation and server completion", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await stubSession(page);
    await stubLearningApi(page);

    await page.goto("/learning");

    await page.route("**/api/v1/learning/lessons/lesson-1/completion", (route) => route.fulfill({
      status: 200,
      contentType: "application/json",
      body: successBody({
        progress: {
          lesson_id: "lesson-1",
          state: "completed",
          completed: true,
          started_at: "2026-08-16T08:00:00Z",
          completed_at: "2026-08-16T08:10:00Z",
          last_accessed_at: "2026-08-16T08:10:00Z",
          updated_at: "2026-08-16T08:10:00Z",
        },
      }),
    }));

    await page.goto("/learning/lessons/lesson-1");

    const next = page.getByRole("link", { name: "Next lesson: Reading a Chart" });
    await expect(next).toHaveAttribute("rel", "next");
    await next.focus();
    await expect(next).toBeFocused();
    await expectNoHorizontalOverflow(page);
    await expectAccessible(page);

    const complete = page.getByRole("button", { name: "Mark lesson complete" });
    await complete.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("status")).toContainText("Completion saved.");
  });

  test("recovers from a course API failure", async ({ page }) => {
    await stubSession(page);
    let attempts = 0;
    await page.route(`**/api/v1/learning/courses/${courseId}`, (route) => {
      attempts += 1;
      return route.fulfill(attempts === 1
        ? {
            status: 503,
            contentType: "application/json",
            body: JSON.stringify({
              api_version: 1,
              request_id: "learning-e2e-error",
              error: { code: "SERVICE_UNAVAILABLE", message: "learning unavailable" },
            }),
          }
        : {
            status: 200,
            contentType: "application/json",
            body: successBody(course),
          });
    });

    await page.goto("/learning");

    const retry = page.getByRole("button", { name: "Try again" });
    await retry.focus();
    await expect(retry).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { name: "Beginner Trading" })).toBeVisible();
  });

  test("renders an empty published-course state", async ({ page }) => {
    await stubSession(page);
    await stubLearningApi(page, { ...course, modules: [] });

    await page.goto("/learning");

    await expect(page.getByRole("status")).toContainText("No published modules are available yet.");
  });
});
