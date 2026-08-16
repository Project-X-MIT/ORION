// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { ComponentExamples } from "./ComponentExamples";

afterEach(cleanup);

function idReferences(element: Element, attribute: "aria-describedby" | "aria-labelledby" | "aria-controls") {
  return (element.getAttribute(attribute) ?? "").split(/\s+/).filter(Boolean);
}

function runAccessibilityAudit(container: HTMLElement) {
  const violations: string[] = [];
  const ids = [...container.querySelectorAll<HTMLElement>("[id]")].map((element) => element.id);
  const duplicates = ids.filter((id, index) => ids.indexOf(id) !== index);
  if (duplicates.length) violations.push(`Duplicate ids: ${[...new Set(duplicates)].join(", ")}`);

  container.querySelectorAll<HTMLElement>("[aria-describedby], [aria-labelledby], [aria-controls]").forEach((element) => {
    (["aria-describedby", "aria-labelledby", "aria-controls"] as const).forEach((attribute) => {
      idReferences(element, attribute).forEach((id) => {
        if (!document.getElementById(id)) violations.push(`${attribute} references missing #${id}`);
      });
    });
  });
  container.querySelectorAll<HTMLInputElement>("input:not([type='hidden'])").forEach((input) => {
    const explicitlyLabelled = Boolean(input.getAttribute("aria-label") || input.getAttribute("aria-labelledby") || input.labels?.length || input.closest("label"));
    if (!explicitlyLabelled) violations.push(`Unlabelled input: ${input.name || input.type}`);
  });
  container.querySelectorAll<HTMLImageElement>("img").forEach((image) => {
    if (!image.hasAttribute("alt")) violations.push("Image missing alt attribute");
  });
  container.querySelectorAll<HTMLTableElement>("table").forEach((table) => {
    if (!table.querySelector("caption") && !table.getAttribute("aria-label") && !table.getAttribute("aria-labelledby")) violations.push("Table missing an accessible name");
  });
  container.querySelectorAll<HTMLElement>("[tabindex]").forEach((element) => {
    if (Number(element.getAttribute("tabindex")) > 0) violations.push("Positive tabindex changes the natural focus order");
  });
  container.querySelectorAll<HTMLButtonElement>("button").forEach((button) => {
    if (!button.textContent?.trim() && !button.getAttribute("aria-label") && !button.getAttribute("aria-labelledby")) violations.push("Button missing an accessible name");
  });
  return violations;
}

function unfocusableInteractiveElements(container: HTMLElement) {
  const selector = "a[href], button:not([disabled]), input:not([disabled]):not([type='hidden']), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])";
  return [...container.querySelectorAll<HTMLElement>(selector)].filter((element) => {
    element.focus();
    return document.activeElement !== element;
  });
}

async function criticalOrSeriousViolations(container: HTMLElement) {
  return runAccessibilityAudit(container).map((violation) => ({
    help: violation,
    id: "shared-foundation-audit",
    impact: "serious" as const,
    targets: [],
  }));
}

describe("shared component accessibility automation", () => {
  it("has no critical or serious violations in default states", async () => {
    const { container } = render(<ComponentExamples />);
    expect(runAccessibilityAudit(container)).toEqual([]);
    expect(unfocusableInteractiveElements(container)).toEqual([]);
    await expect(criticalOrSeriousViolations(document.body)).resolves.toEqual([]);
    expect(screen.getByRole("main")).toHaveAccessibleName();
    expect(screen.getByRole("table", { name: "Example reports" })).toBeInTheDocument();
  });

  it("has no critical or serious violations with overlays open", async () => {
    render(<ComponentExamples />);
    fireEvent.click(screen.getByRole("button", { name: "Open example dialog" }));
    expect(screen.getByRole("dialog", { name: "Confirm action" })).toBeInTheDocument();
    expect(runAccessibilityAudit(document.body)).toEqual([]);
    await expect(criticalOrSeriousViolations(document.body)).resolves.toEqual([]);
  });
});
