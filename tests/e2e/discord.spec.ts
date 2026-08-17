import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// TODO(Div): enable after the Discord feature is mounted in the shared
// application router and the approved runtime configuration is registered.
const discordE2eEnabled = process.env.ORION_DISCORD_E2E_ENABLED === "1";
const inviteUrl = "https://discord.gg/qXRjY4PPp";
const viewports = [
  { name: "mobile", width: 390, height: 844 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "desktop", width: 1280, height: 900 },
] as const;

test.describe("Discord community link", () => {
  test.skip(
    !discordE2eEnabled,
    "Set ORION_DISCORD_E2E_ENABLED=1 after Div mounts the Discord feature and registers config.",
  );

  for (const viewport of viewports) {
    test(`is keyboard accessible, protected, and responsive on ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await page.goto("/discord");

      await expect(page.getByRole("heading", { name: "Join the ORION community" })).toBeVisible();
      const link = page.getByRole("link", { name: "Join ORION on Discord" });
      await expect(link).toHaveAttribute("href", inviteUrl);
      await expect(link).toHaveAttribute("target", "_blank");
      await expect(link).toHaveAttribute("rel", "noopener noreferrer");
      await expect(link).toHaveAttribute("referrerPolicy", "no-referrer");
      await link.focus();
      await expect(link).toBeFocused();

      const widths = await page.evaluate(() => ({
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
      }));
      expect(widths.documentWidth).toBeLessThanOrEqual(widths.viewportWidth);

      const results = await new AxeBuilder({ page }).analyze();
      expect(results.violations).toEqual([]);
    });
  }
});
