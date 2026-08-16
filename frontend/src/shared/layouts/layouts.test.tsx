// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AuthLayout } from "./AuthLayout";
import { DashboardLayout } from "./DashboardLayout";
import { PublicLayout } from "./PublicLayout";

afterEach(cleanup);

describe("shared layouts", () => {
  it("provides public landmarks, active navigation, and an expandable menu", () => {
    render(<PublicLayout currentPath="/about" footer="Copyright" navItems={[
      { exact: true, href: "/", label: "Home" },
      { href: "/about", label: "About" },
    ]}><h1>Welcome</h1></PublicLayout>);

    expect(screen.getByRole("link", { name: "Skip to main content" })).toHaveAttribute("href", "#main-content");
    expect(screen.getByRole("link", { name: "About" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("main")).toContainElement(screen.getByRole("heading", { name: "Welcome" }));
    expect(screen.getByRole("contentinfo")).toHaveTextContent("Copyright");

    const menu = screen.getByRole("button", { name: "Toggle navigation" });
    expect(menu).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(menu);
    expect(menu).toHaveAttribute("aria-expanded", "true");
  });

  it("labels auth content and exposes supporting content", () => {
    render(<AuthLayout aside="Learn with confidence" footer="Need help?" subtitle="Use your account" title="Sign in"><form aria-label="Login" /></AuthLayout>);

    expect(screen.getByRole("heading", { name: "Sign in", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "About ORION" })).toHaveTextContent("Learn with confidence");
    expect(screen.getByRole("main")).toContainElement(screen.getByRole("form", { name: "Login" }));
    expect(screen.getByText("Need help?")).toBeInTheDocument();
  });

  it("controls dashboard navigation and sign-out accessibly", () => {
    const onSignOut = vi.fn();
    render(<DashboardLayout currentPath="/research/active" navItems={[
      { href: "/", label: "Overview" },
      { href: "/research", label: "Research" },
    ]} onSignOut={onSignOut} pageTitle="Workspace" user={{ name: "Ada Lovelace", secondaryText: "Researcher" }}><h1>Reports</h1></DashboardLayout>);

    expect(screen.getByRole("link", { name: "Research" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("navigation", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Ada Lovelace" })).toBeInTheDocument();

    const open = screen.getByRole("button", { name: "Open navigation" });
    fireEvent.click(open);
    expect(open).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(screen.getAllByRole("button", { name: "Close navigation" })[0]);
    expect(open).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(onSignOut).toHaveBeenCalledOnce();
  });
});
