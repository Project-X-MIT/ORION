// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Button } from "./Button";
import { Card } from "./Card";
import { Input } from "./Input";
import { Modal } from "./Modal";
import { Pagination } from "./Pagination";
import { Tabs } from "./Tabs";
import { Tooltip } from "./Tooltip";

afterEach(cleanup);

describe("shared UI behavior", () => {
  it("disables a loading button and exposes its status", () => {
    render(<Button isLoading loadingLabel="Saving">Save</Button>);

    expect(screen.getByRole("button", { name: "Saving" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Saving");
  });

  it("connects an input error to the native field", () => {
    render(<Input error="Email is required" label="Email" />);

    const input = screen.getByLabelText("Email");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("Email is required");
    expect(screen.getByRole("alert")).toHaveTextContent("Email is required");
  });

  it("focuses, closes, and restores focus for a modal", () => {
    const close = vi.fn();
    const { rerender } = render(<><button>Open</button><Modal isOpen={false} onClose={close} title="Settings"><button>Save</button></Modal></>);
    screen.getByRole("button", { name: "Open" }).focus();

    rerender(<><button>Open</button><Modal isOpen onClose={close} title="Settings"><button>Save</button></Modal></>);
    const closeButton = screen.getByRole("button", { name: "Close dialog" });
    const saveButton = screen.getByRole("button", { name: "Save" });
    expect(closeButton).toHaveFocus();

    saveButton.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(closeButton).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(saveButton).toHaveFocus();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(close).toHaveBeenCalledOnce();

    rerender(<><button>Open</button><Modal isOpen={false} onClose={close} title="Settings"><button>Save</button></Modal></>);
    expect(screen.getByRole("button", { name: "Open" })).toHaveFocus();
  });

  it("supports arrow-key tab selection and skips disabled tabs", () => {
    render(<Tabs items={[
      { id: "one", label: "One", content: "First" },
      { id: "two", label: "Two", content: "Second", disabled: true },
      { id: "three", label: "Three", content: "Third" },
    ]} />);

    const first = screen.getByRole("tab", { name: "One" });
    first.focus();
    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Three" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Third");
  });

  it("shows a tooltip for keyboard focus", () => {
    render(<Tooltip content="Helpful context"><button>Help</button></Tooltip>);
    fireEvent.focus(screen.getByRole("button", { name: "Help" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Helpful context");
    expect(screen.getByRole("button", { name: "Help" })).toHaveAccessibleDescription("Helpful context");
  });

  it("makes non-native tooltip triggers keyboard focusable", () => {
    render(<Tooltip content="Definition"><span>Term</span></Tooltip>);
    const trigger = screen.getByText("Term");
    expect(trigger).toHaveAttribute("tabindex", "0");
    fireEvent.focus(trigger);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Definition");
  });

  it("activates an interactive card with Enter and Space", () => {
    const activate = vi.fn();
    render(<Card onActivate={activate}>Open report</Card>);
    const card = screen.getByRole("button", { name: "Open report" });
    card.focus();
    fireEvent.keyDown(card, { key: "Enter" });
    fireEvent.keyDown(card, { key: " " });
    expect(activate).toHaveBeenCalledTimes(2);
  });

  it("enforces pagination boundaries and announces the current page", () => {
    const change = vi.fn();
    render(<Pagination onPageChange={change} page={1} pageCount={10} />);
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Page 1" })).toHaveAttribute("aria-current", "page");
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(change).toHaveBeenCalledWith(2);
  });
});
