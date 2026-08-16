// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DataTable } from "./DataTable";
import type { DataTableColumn } from "./DataTable";

interface Person { id: string; name: string; score: number }
const rows: Person[] = [{ id: "a", name: "Ada", score: 10 }, { id: "g", name: "Grace", score: 20 }];
const columns: DataTableColumn<Person>[] = [
  { id: "name", header: "Name", render: (row) => row.name, sortable: true },
  { align: "right", id: "score", header: "Score", render: (row) => row.score },
];

afterEach(cleanup);

describe("DataTable", () => {
  it("renders native semantics and cycles sorting", () => {
    const onSortChange = vi.fn();
    const { rerender } = render(<DataTable caption="People" columns={columns} getRowId={(row) => row.id} onSortChange={onSortChange} rows={rows} />);

    expect(screen.getByRole("table", { name: "People" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Name/ }));
    expect(onSortChange).toHaveBeenLastCalledWith({ columnId: "name", direction: "ascending" });

    rerender(<DataTable caption="People" columns={columns} getRowId={(row) => row.id} onSortChange={onSortChange} rows={rows} sort={{ columnId: "name", direction: "ascending" }} />);
    expect(screen.getByRole("columnheader", { name: /Name/ })).toHaveAttribute("aria-sort", "ascending");
    fireEvent.click(screen.getByRole("button", { name: /Name/ }));
    expect(onSortChange).toHaveBeenLastCalledWith({ columnId: "name", direction: "descending" });
  });

  it("selects one row and all rows without triggering row activation", () => {
    const onSelectionChange = vi.fn();
    const onRowClick = vi.fn();
    render(<DataTable caption="People" columns={columns} getRowId={(row) => row.id} getRowLabel={(row) => row.name} onRowClick={onRowClick} onSelectionChange={onSelectionChange} rows={rows} />);

    fireEvent.click(screen.getByRole("checkbox", { name: "Select Ada" }));
    expect(onSelectionChange.mock.calls[0][0]).toEqual(new Set(["a"]));
    expect(onRowClick).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select all rows on this page" }));
    expect(onSelectionChange.mock.calls[1][0]).toEqual(new Set(["a", "g"]));
  });

  it("activates clickable rows from the keyboard", () => {
    const onRowClick = vi.fn();
    render(<DataTable caption="People" columns={columns} getRowId={(row) => row.id} getRowLabel={(row) => row.name} onRowClick={onRowClick} rows={rows} />);

    fireEvent.keyDown(screen.getByRole("row", { name: "Ada" }), { key: "Enter" });
    expect(onRowClick).toHaveBeenCalledWith(rows[0]);
  });

  it("announces error and renders empty and loading lifecycle states", () => {
    const { rerender } = render(<DataTable caption="People" columns={columns} error="Could not load people" getRowId={(row) => row.id} rows={[]} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Could not load people");

    rerender(<DataTable caption="People" columns={columns} emptyMessage="Nobody here" getRowId={(row) => row.id} rows={[]} />);
    expect(screen.getByRole("cell", { name: "Nobody here" })).toHaveAttribute("colspan", "2");

    rerender(<DataTable caption="People" columns={columns} getRowId={(row) => row.id} isLoading loadingRowCount={2} rows={[]} />);
    expect(screen.getByRole("region")).toHaveAttribute("aria-busy", "true");
    expect(document.querySelectorAll("tbody tr")).toHaveLength(2);
  });
});
