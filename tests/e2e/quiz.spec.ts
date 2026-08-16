import { expect, test } from "@playwright/test";

const requestId = "00000000-0000-4000-8000-000000000099";
const user = {
  id: "00000000-0000-4000-8000-000000000050",
  email: "quiz@example.test",
  username: "quiz-user",
  display_name: "Quiz User",
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

test.describe("quiz experiences", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/v1/auth/me", (route) => route.fulfill(success({ user })));
    await page.goto("/quiz");
    await expect(page.getByRole("heading", { name: "Choose your quiz" })).toBeVisible();
  });

  test("supports keyboard mode selection and Basic MCQ submission", async ({ page }) => {
    let submissions = 0;
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill(success({
          items: [{
            id: "00000000-0000-4000-8000-000000000001",
            category: "science",
            question_text: "What is the chemical symbol for water?",
            options: [
              { id: "00000000-0000-4000-8000-000000001001", option_text: "H2O", position: 0 },
              { id: "00000000-0000-4000-8000-000000001002", option_text: "CO2", position: 1 },
            ],
          }],
          limit: 20,
          offset: 0,
          has_more: false,
        }));
        return;
      }

      submissions += 1;
      await route.fulfill(success({
        attempt: {
          id: "00000000-0000-4000-8000-000000000101",
          status: "completed",
          total_questions: 1,
          correct_answers: 1,
          score: 100,
          rating_before: 500,
          rating_after: 512,
          started_at: "2026-08-16T08:00:00Z",
          completed_at: "2026-08-16T08:00:10Z",
        },
        rating: { rating: 512, games_played: 1, wins: 1, losses: 0, draws: 0 },
        answers: [{
          question_id: "00000000-0000-4000-8000-000000000001",
          correct: true,
          rating_delta: 12,
        }],
      }));
    });

    const basicMode = page.getByTestId("quiz-mode-basic");
    await basicMode.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { name: "Basic Quiz" })).toBeVisible();

    await page.getByLabel("H2O").focus();
    await page.keyboard.press("Space");
    await expect(page.getByLabel("H2O")).toBeChecked();
    const submit = page.getByTestId("quiz-submit");
    await submit.focus();
    await Promise.allSettled([page.keyboard.press("Enter"), submit.dispatchEvent("click")]);

    await expect(page.getByTestId("quiz-result")).toContainText("Server score: 100%");
    await expect(page.getByTestId("quiz-result")).toContainText("Rating change: +12");
    expect(submissions).toBe(1);
  });

  test("retries an ambiguous network failure with the same attempt id", async ({ page }) => {
    const attemptIds: string[] = [];
    let submissionRequests = 0;
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill(success({
          items: [{
            id: "00000000-0000-4000-8000-000000000004",
            category: "science",
            question_text: "Which gas do plants absorb?",
            options: [{ id: "00000000-0000-4000-8000-000000001004", option_text: "Carbon dioxide", position: 0 }],
          }],
          limit: 20,
          offset: 0,
          has_more: false,
        }));
        return;
      }

      submissionRequests += 1;
      const body = route.request().postDataJSON() as { attempt_id: string };
      attemptIds.push(body.attempt_id);
      if (submissionRequests === 1) {
        await route.abort("failed");
        return;
      }
      await route.fulfill(success({
        attempt: {
          id: body.attempt_id,
          status: "completed",
          total_questions: 1,
          correct_answers: 1,
          score: 100,
          rating_before: 500,
          rating_after: 507,
          started_at: "2026-08-16T08:00:00Z",
          completed_at: "2026-08-16T08:00:10Z",
        },
        rating: { rating: 507, games_played: 1, wins: 1, losses: 0, draws: 0 },
        answers: [{ question_id: "00000000-0000-4000-8000-000000000004", correct: true, rating_delta: 7 }],
      }));
    });

    await page.getByTestId("quiz-mode-basic").click();
    await page.getByLabel("Carbon dioxide").check();
    const submit = page.getByTestId("quiz-submit");
    await submit.click();
    await expect(page.getByRole("alert")).toContainText("could not be reached");

    await submit.click();
    await expect(page.getByText("Server score: 100%")).toBeVisible();
    expect(submissionRequests).toBe(2);
    expect(attemptIds[0]).toBe(attemptIds[1]);
  });

  test("validates incomplete answers before making a request", async ({ page }) => {
    let submissions = 0;
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill(success({
          items: [{
            id: "00000000-0000-4000-8000-000000000002",
            category: "geography",
            question_text: "Which city is the capital of Japan?",
            options: [{ id: "00000000-0000-4000-8000-000000001003", option_text: "Tokyo", position: 0 }],
          }],
          limit: 20,
          offset: 0,
          has_more: false,
        }));
        return;
      }
      submissions += 1;
      await route.fulfill(success({}));
    });

    await page.getByTestId("quiz-mode-basic").click();
    await page.getByTestId("quiz-submit").click();
    await expect(page.getByRole("alert")).toContainText("Answer every question");
    expect(submissions).toBe(0);
  });

  test("shows pending Advanced settlement without inventing a score", async ({ page }) => {
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill(success({
          items: [{
            id: "00000000-0000-4000-8000-000000000003",
            category: "markets",
            question_text: "What will the reference value be at the horizon?",
            input_type: "numeric",
            value_spec: { unit_code: "price", currency_code: "USD", scale: 2 },
            options: [],
          }],
          limit: 20,
          offset: 0,
          has_more: false,
        }));
        return;
      }
      await route.fulfill(success({
        attempt: {
          id: "00000000-0000-4000-8000-000000000102",
          status: "pending",
          total_questions: 1,
          correct_answers: 0,
          score: 0,
          rating_before: 500,
          rating_after: 500,
          started_at: "2026-08-16T08:00:00Z",
          completed_at: null,
        },
        rating: { rating: 500, games_played: 0, wins: 0, losses: 0, draws: 0 },
        predictions: [],
      }));
    });

    await page.getByTestId("quiz-mode-advanced").click();
    await page.getByRole("spinbutton", { name: /Enter your prediction/ }).fill("123.45");
    await page.getByTestId("quiz-submit").click();

    await expect(page.getByRole("heading", { name: "Advanced settlement is pending" })).toBeVisible();
    await expect(page.getByText("We will not estimate a score or rating change")).toBeVisible();
  });

  test("renders settled Advanced score and rating from the server", async ({ page }) => {
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill(success({
          items: [{
            id: "00000000-0000-4000-8000-000000000005",
            category: "markets",
            question_text: "What will the reference value be at the horizon?",
            input_type: "numeric",
            value_spec: { unit_code: "price", currency_code: "USD", scale: 2, min: "0", max: "1000", step: "0.01" },
            options: [],
          }],
          limit: 20,
          offset: 0,
          has_more: false,
        }));
        return;
      }
      await route.fulfill(success({
        attempt: {
          id: "00000000-0000-4000-8000-000000000103",
          status: "completed",
          total_questions: 1,
          correct_answers: 1,
          score: 100,
          rating_before: 700,
          rating_after: 713,
          started_at: "2026-08-16T08:00:00Z",
          completed_at: "2026-08-16T08:00:10Z",
        },
        rating: { rating: 713, games_played: 4, wins: 3, losses: 1, draws: 0 },
        predictions: [{ question_id: "00000000-0000-4000-8000-000000000005", correct: true, rating_delta: 13 }],
      }));
    });

    await page.getByTestId("quiz-mode-advanced").focus();
    await page.keyboard.press("Enter");
    await page.getByRole("spinbutton", { name: /Enter your prediction/ }).fill("123.45");
    await page.getByTestId("quiz-submit").focus();
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("quiz-result")).toContainText("Server score: 100%");
    await expect(page.getByTestId("quiz-result")).toContainText("Rating change: +13");
    await expect(page.getByTestId("quiz-result")).toContainText("Server rating: 713");
  });

  test("recovers from a question loading failure", async ({ page }) => {
    let loads = 0;
    await page.route("**/api/v1/quiz/**", async (route) => {
      if (route.request().method() !== "GET") {
        await route.continue();
        return;
      }
      loads += 1;
      if (loads === 1) {
        await route.fulfill(failure("SERVICE_UNAVAILABLE", "Quiz questions are temporarily unavailable", 503));
        return;
      }
      await route.fulfill(success({ items: [], limit: 20, offset: 0, has_more: false }));
    });

    await page.getByTestId("quiz-mode-basic").click();
    await expect(page.getByRole("alert")).toContainText("temporarily unavailable");
    await page.getByRole("button", { name: "Try again" }).click();
    await expect(page.getByRole("status")).toContainText("no active questions");
    expect(loads).toBe(2);
  });

  for (const viewport of [360, 768, 1440]) {
    test(`has no horizontal overflow at ${viewport}px`, async ({ page }) => {
      await page.setViewportSize({ width: viewport, height: 900 });
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
      expect(overflow).toBe(false);
    });
  }
});
