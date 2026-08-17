import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";

const viewports = [
  { name: "mobile", width: 360, height: 800 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "desktop", width: 1440, height: 900 },
] as const;

async function expectNoPageOverflow(page: Page) {
  const measurement = await page.evaluate(() => {
    const root = document.documentElement;
    const offenders = [...document.body.querySelectorAll<HTMLElement>("*")]
      .filter((element) => {
        const style = getComputedStyle(element);
        if (style.display === "none" || style.visibility === "hidden") return false;
        if (element.closest(".ui-data-table__scroll, .ui-skip-link")) return false;
        const bounds = element.getBoundingClientRect();
        return bounds.width > 0 && (bounds.left < -0.5 || bounds.right > window.innerWidth + 0.5);
      })
      .map((element) => {
        const bounds = element.getBoundingClientRect();
        return {
          className: element.className,
          left: Math.round(bounds.left),
          right: Math.round(bounds.right),
          tag: element.tagName.toLowerCase(),
          width: Math.round(bounds.width),
        };
      });
    return {
      clientWidth: root.clientWidth,
      offenders,
      scrollWidth: root.scrollWidth,
    };
  });

  expect(measurement.offenders, JSON.stringify(measurement.offenders, null, 2)).toEqual([]);
  expect(measurement.scrollWidth).toBe(measurement.clientWidth);
}

async function expectNoCriticalOrSeriousViolations(page: Page) {
  const results = await new AxeBuilder({ page })
    .disableRules(["color-contrast"])
    .analyze();
  expect(results.violations.filter((violation) => violation.impact === "critical" || violation.impact === "serious")).toEqual([]);
}

for (const viewport of viewports) {
  test(`${viewport.name} (${viewport.width}px) has no page overflow`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto("/component-examples.html");
    await page.locator("main").waitFor();
    await page.evaluate(() => document.fonts.ready);

    await expectNoPageOverflow(page);
    await expectNoCriticalOrSeriousViolations(page);

    await page.getByRole("button", { name: "Open example dialog" }).click();
    await expect(page.getByRole("dialog", { name: "Confirm action" })).toBeVisible();
    await expectNoPageOverflow(page);
    await expectNoCriticalOrSeriousViolations(page);
  });
}
