import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

type LifecycleState =
  | "draft"
  | "submitted"
  | "under_review"
  | "approved"
  | "rejected"
  | "published"
  | "awarded";

type Credentials = {
  email: string;
  password: string;
};

type ResearchE2EFixture = {
  frontendUrl?: string;
  author: Credentials;
  reviewer: Credentials;
  lifecycle: Record<LifecycleState, string>;
};

type BackendPaperProjection = {
  status: string;
  content: string;
  published_at: string | null;
  elo_award: number | null;
  elo_awarded: boolean;
};

const lifecycleStates: Array<{ state: LifecycleState; label: string }> = [
  { state: "draft", label: "Draft" },
  { state: "submitted", label: "Submitted" },
  { state: "under_review", label: "Under review" },
  { state: "approved", label: "Approved" },
  { state: "rejected", label: "Rejected" },
  { state: "published", label: "Published" },
  { state: "awarded", label: "Awarded" },
];

function loadFixture(): ResearchE2EFixture | null {
  const raw = process.env.RESEARCH_E2E_FIXTURE;
  if (!raw) return null;

  try {
    const fixture = JSON.parse(raw) as ResearchE2EFixture;
    const complete = lifecycleStates.every(({ state }) => Boolean(fixture.lifecycle?.[state]));
    if (!fixture.author?.email || !fixture.author.password ||
        !fixture.reviewer?.email || !fixture.reviewer.password || !complete) {
      throw new Error("RESEARCH_E2E_FIXTURE is missing credentials or lifecycle paper IDs");
    }
    return fixture;
  } catch (error) {
    throw new Error(
      `RESEARCH_E2E_FIXTURE must be valid JSON with author, reviewer, and all lifecycle IDs: ${String(error)}`,
    );
  }
}

const fixture = loadFixture();

function appUrl(path: string): string {
  const base = fixture?.frontendUrl ?? process.env.RESEARCH_E2E_FRONTEND_URL ?? "http://127.0.0.1:5173";
  return `${base.replace(/\/$/u, "")}${path}`;
}

async function signIn(page: Page, credentials: Credentials) {
  await page.goto(appUrl("/login"));
  await page.getByLabel("Email").fill(credentials.email);
  await page.getByLabel("Password").fill(credentials.password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/$/u);
}

async function failNextResearchRequest(page: Page, pathSuffix: string, message: string) {
  let failed = false;
  await page.route("**/api/v1/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (!failed && request.method() === "POST" && url.pathname.endsWith(pathSuffix)) {
      failed = true;
      await route.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          api_version: 1,
          request_id: "00000000-0000-0000-0000-000000000098",
          error: { code: "SERVICE_UNAVAILABLE", message },
        }),
      });
      return;
    }
    await route.continue();
  });
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

function readPaperId(value: string | null): string {
  const paperId = value?.trim() ?? "";
  if (!/^[0-9a-f-]{36}$/iu.test(paperId)) {
    throw new Error("The research UI did not render a valid persisted paper ID");
  }
  return paperId;
}

async function expectLifecycleState(page: Page, state: LifecycleState, paperId: string) {
  const label = lifecycleStates.find((item) => item.state === state)?.label;
  if (!label) throw new Error(`Unknown research lifecycle state: ${state}`);

  await page.goto(appUrl(`/research/${encodeURIComponent(paperId)}`));
  await expect(page.getByRole("heading", { name: "Research status" })).toBeVisible();
  await expect(page.locator(`[aria-label="Status: ${label}"]`).first()).toBeVisible();

  if (state === "published" || state === "awarded") {
    await expect(page.getByRole("heading", { name: "Publication details" })).toBeVisible();
  }
  if (state === "rejected") {
    await expect(page.getByText("changes requested", { exact: false }).first()).toBeVisible();
    await expect(page.getByRole("button", { name: "Create revision" })).toBeVisible();
  }
}

async function loadBackendPaper(page: Page, paperId: string): Promise<BackendPaperProjection> {
  const responsePromise = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return response.request().method() === "GET" &&
      url.pathname.endsWith(`/research/${paperId}`) &&
      response.status() === 200;
  });
  await page.goto(appUrl(`/research/${encodeURIComponent(paperId)}`));
  const response = await responsePromise;
  const envelope = await response.json() as { data: BackendPaperProjection };
  return envelope.data;
}

test.describe("research lifecycle against the backend", () => {
  test.skip(
    !fixture,
    "Set RESEARCH_E2E_FIXTURE with real author/reviewer credentials and backend paper IDs to run this suite.",
  );

  test("authors can read every backend-owned lifecycle state", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);

    for (const { state } of lifecycleStates) {
      await expectLifecycleState(page, state, fixture.lifecycle[state]);
    }
  });

  test("refresh and relogin preserve the backend-owned status", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    const paperId = fixture.lifecycle.approved;

    await signIn(page, fixture.author);
    await expectLifecycleState(page, "approved", paperId);

    await page.reload();
    await expect(page.locator('[aria-label="Status: Approved"]').first()).toBeVisible();

    await page.context().clearCookies();
    await signIn(page, fixture.author);
    await expectLifecycleState(page, "approved", paperId);
    await expect(page.locator('[aria-label="Status: Published"]').first()).toHaveCount(0);
  });

  test("keyboard users can operate the editor and submission dialog without horizontal overflow", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl("/research"));
    await expectNoHorizontalOverflow(page);
    await expectAccessible(page);

    const title = page.getByLabel("Title");
    const content = page.getByLabel("Paper content");
    await title.focus();
    await expect(title).toBeFocused();
    await page.keyboard.type(`Keyboard smoke draft ${Date.now()}`);
    await content.focus();
    await expect(content).toBeFocused();
    await page.keyboard.type("The editor remains operable without a pointer.");
    await expect(title).toHaveAttribute("aria-describedby", /.+/u);
    await expect(content).toHaveAttribute("aria-describedby", /.+/u);

    const submitButton = page.getByRole("button", { name: "Save and submit for review" });
    await submitButton.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Submit this paper for review?" });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute("aria-modal", "true");
    await expectNoHorizontalOverflow(page);
    await expectAccessible(page);
    const confirmButton = dialog.getByRole("button", { name: "Confirm and submit for review" });
    await expect(confirmButton).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(submitButton).toBeFocused();
  });

  test("a failed draft save can be retried without losing entered content", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl("/research"));
    await failNextResearchRequest(page, "/research", "draft saving is temporarily unavailable");

    const title = `Recoverable backend draft ${Date.now()}`;
    const content = "This content must survive the failed save and retry.";
    await page.getByLabel("Title").fill(title);
    await page.getByLabel("Paper content").fill(content);
    await page.getByRole("button", { name: "Save draft" }).click();
    await expect(page.getByRole("alert")).toContainText("draft saving is temporarily unavailable");
    await expect(page.getByLabel("Title")).toHaveValue(title);
    await expect(page.getByLabel("Paper content")).toHaveValue(content);

    await page.getByRole("button", { name: "Retry save" }).click();
    await expect(page.getByRole("heading", { name: "Edit research draft" })).toBeVisible();
    await expect(page.getByLabel("Title")).toHaveValue(title);
    await expect(page.getByLabel("Paper content")).toHaveValue(content);
  });

  test("a failed submission can be retried without losing the saved draft", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl("/research"));

    const title = `Recoverable backend submission ${Date.now()}`;
    const content = "This saved content must survive a failed submission.";
    await page.getByLabel("Title").fill(title);
    await page.getByLabel("Paper content").fill(content);
    await page.getByRole("button", { name: "Save draft" }).click();
    await expect(page.getByRole("heading", { name: "Edit research draft" })).toBeVisible();
    await failNextResearchRequest(page, "/submission", "submission is temporarily unavailable");

    await page.getByRole("button", { name: "Save and submit for review" }).click();
    await expect(page.getByRole("dialog", { name: "Submit this paper for review?" })).toBeVisible();
    await page.getByRole("button", { name: "Confirm and submit for review" }).click();
    await expect(page.getByRole("alert")).toContainText("submission is temporarily unavailable");
    await expect(page.getByRole("dialog", { name: "Submit this paper for review?" })).toBeVisible();
    await page.getByRole("button", { name: "Retry submission" }).click();

    await expect(page.getByRole("heading", { name: "Submitted research" })).toBeVisible();
    await expect(page.getByRole("heading", { name: title })).toBeVisible();
    await expect(page.getByText(content)).toBeVisible();
  });

  test("screen readers can locate rejection feedback and its review structure", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl(`/research/${encodeURIComponent(fixture.lifecycle.rejected)}`));

    const feedback = page.getByRole("region", { name: "Review status and feedback" });
    await expect(feedback).toBeVisible();
    await expect(feedback.getByRole("heading", { name: "Review status and feedback", level: 2 })).toBeVisible();
    await expect(feedback.getByRole("alert")).toContainText("Changes requested");
    await expect(feedback.getByRole("heading", { name: "Reviewer feedback", level: 4 })).toBeVisible();
    await expect(feedback.getByRole("heading", { name: "Evaluation details", level: 4 })).toBeVisible();
    await expect(feedback.getByRole("heading", { name: "Strengths", level: 5 })).toBeVisible();
    await expect(feedback.getByRole("heading", { name: "Concerns", level: 5 })).toBeVisible();
  });

  test("anonymous readers browse the backend publication and award projection", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await page.goto(appUrl("/research"));

    await expect(page.getByRole("heading", { name: "Published research" })).toBeVisible();
    const paperLink = page.locator(`a[href="/research/${encodeURIComponent(fixture.lifecycle.published)}"]`);
    await expect(paperLink).toBeVisible();
    await paperLink.focus();
    await expect(paperLink).toBeFocused();
    await paperLink.click();
    await expect(page.getByRole("heading", { name: "Publication details" })).toBeVisible();

    await page.goto(appUrl(`/research/${encodeURIComponent(fixture.lifecycle.awarded)}`));
    await expect(page.getByRole("heading", { name: "Publication details" })).toBeVisible();
    await expect(page.getByText(/Awarded rating:/u).first()).toBeVisible();
  });

  test("the displayed award belongs to the published evaluated paper version", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");

    const pendingPaper = await loadBackendPaper(page, fixture.lifecycle.published);
    expect(pendingPaper.status).toBe("published");
    expect(pendingPaper.published_at).not.toBeNull();
    expect(pendingPaper.elo_awarded).toBe(false);
    expect(pendingPaper.elo_award).toBeNull();
    await expect(page.getByRole("region", { name: "Publication details" }))
      .toContainText("Awarded rating: pending");
    await expect(page.getByLabel("Research paper content")).toContainText(pendingPaper.content);

    const awardedPaper = await loadBackendPaper(page, fixture.lifecycle.awarded);
    expect(awardedPaper.status).toBe("published");
    expect(awardedPaper.published_at).not.toBeNull();
    expect(awardedPaper.elo_awarded).toBe(true);
    expect(awardedPaper.elo_award).not.toBeNull();
    const awardValue = awardedPaper.elo_award ?? 0;
    const awardLabel = `${awardValue > 0 ? "+" : ""}${awardValue}`;
    const publication = page.getByRole("region", { name: "Publication details" });
    await expect(publication).toContainText(`Awarded rating: ${awardLabel} Elo points`);
    await expect(page.getByLabel("Research paper content")).toContainText(awardedPaper.content);
  });

  test("a real draft is submitted and becomes immutable", async ({ page }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl("/research"));

    const title = `Backend lifecycle draft ${Date.now()}`;
    await page.getByLabel("Title").fill(title);
    await page.getByLabel("Paper content").fill("This paper is persisted by the research API.");
    await page.getByRole("button", { name: "Save draft" }).click();
    await expect(page.getByRole("heading", { name: "Edit research draft" })).toBeVisible();

    await page.getByRole("button", { name: "Save and submit for review" }).click();
    await expect(page.getByRole("dialog", { name: "Submit this paper for review?" })).toBeVisible();
    await page.getByRole("button", { name: "Confirm and submit for review" }).click();

    await expect(page.getByRole("heading", { name: "Submitted research" })).toBeVisible();
    const submittedPaperId = readPaperId(await page.locator("code").last().textContent());
    await page.goto(appUrl(`/research/${submittedPaperId}`));
    await expect(page.locator('[aria-label="Status: Submitted"]').first()).toBeVisible();
    await expect(page.getByRole("button", { name: "Save draft" })).toHaveCount(0);
    await expect(page.getByLabel("Title")).toHaveCount(0);
  });

  test("a reviewer completes a real rubric and the author sees the backend decision", async ({
    page,
    browser,
  }) => {
    if (!fixture) throw new Error("Research E2E fixture was not loaded");
    await signIn(page, fixture.author);
    await page.goto(appUrl("/research"));

    const title = `Backend review ${Date.now()}`;
    await page.getByLabel("Title").fill(title);
    await page.getByLabel("Paper content").fill("This paper is submitted to the real reviewer queue.");
    await page.getByRole("button", { name: "Save and submit for review" }).click();
    await page.getByRole("button", { name: "Confirm and submit for review" }).click();
    await expect(page.getByRole("heading", { name: "Submitted research" })).toBeVisible();
    const submittedPaperId = readPaperId(await page.locator("code").last().textContent());

    const reviewerContext = await browser.newContext();
    const reviewerPage = await reviewerContext.newPage();
    try {
      await signIn(reviewerPage, fixture.reviewer);
      await reviewerPage.goto(appUrl("/research"));
      await expect(reviewerPage.getByRole("heading", { name: "Reviewer queue" })).toBeVisible();
      const reviewCard = reviewerPage.locator("li").filter({ hasText: title });
      await reviewCard.getByText("Read paper and complete review").click();
      await reviewerPage.getByLabel("Rationale").fill("The method and evidence are reproducible.");
      await reviewerPage.getByLabel("Reference").fill("Results section");
      await reviewerPage.getByLabel("Finding").fill("The result is supported by the reported experiment.");
      await reviewerPage.getByLabel("Strengths (one per line)").fill("Clear methodology");
      await reviewerPage.getByLabel("Concerns (one per line)").fill("The sample is limited");
      await reviewerPage.getByRole("button", { name: "Submit review" }).click();
      await expect(reviewerPage.getByRole("status")).toContainText("Review submitted");
    } finally {
      await reviewerContext.close();
    }

    await page.goto(appUrl(`/research/${submittedPaperId}`));
    await expect(page.getByRole("heading", { name: "Research status" })).toBeVisible();
    await expect(page.locator('[aria-label="Status: Approved"]').first()).toBeVisible();
  });
});
