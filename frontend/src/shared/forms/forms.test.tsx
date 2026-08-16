// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { FileUploader } from "./FileUploader";
import { Form } from "./Form";
import { NumberField } from "./NumberField";

afterEach(cleanup);

describe("shared forms", () => {
  it("locks fields and announces status while submitting", () => {
    render(<Form aria-label="Profile" error="Could not save" isSubmitting onSubmit={vi.fn()}><input aria-label="Name" /></Form>);

    expect(screen.getByLabelText("Name")).toBeDisabled();
    expect(screen.getByRole("form")).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Could not save");
    expect(screen.getByRole("button", { name: "Submitting" })).toBeDisabled();
  });

  it("increments a number, respects its maximum, and reports values", () => {
    const onValueChange = vi.fn();
    render(<NumberField defaultValue={2} label="Quantity" max={3} onValueChange={onValueChange} />);

    fireEvent.click(screen.getByRole("button", { name: "Increase Quantity" }));
    expect(screen.getByLabelText("Quantity")).toHaveValue(3);
    expect(onValueChange).toHaveBeenLastCalledWith(3);
    fireEvent.click(screen.getByRole("button", { name: "Increase Quantity" }));
    expect(screen.getByLabelText("Quantity")).toHaveValue(3);
    expect(screen.getByRole("button", { name: "Increase Quantity" })).not.toHaveAttribute("tabindex", "-1");
  });

  it("selects, rejects, and removes files accessibly", () => {
    const onFilesChange = vi.fn();
    const onReject = vi.fn();
    render(<FileUploader accept="image/*" label="Evidence" maxSize={10} multiple onFilesChange={onFilesChange} onReject={onReject} />);
    const input = screen.getByLabelText("Evidence");
    const valid = new File(["ok"], "proof.png", { type: "image/png" });
    const wrongType = new File(["text"], "notes.txt", { type: "text/plain" });

    fireEvent.change(input, { target: { files: [valid, wrongType] } });
    expect(onReject).toHaveBeenCalledWith([wrongType], "type");
    expect(screen.getByRole("alert")).toHaveTextContent("1 file was rejected");
    expect(screen.getByRole("list", { name: "Selected files" })).toHaveTextContent("proof.png");

    fireEvent.click(screen.getByRole("button", { name: "Remove proof.png" }));
    expect(onFilesChange).toHaveBeenLastCalledWith([]);
  });
});
