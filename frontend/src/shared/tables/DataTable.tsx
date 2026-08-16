import { useEffect, useRef } from "react";
import type { CSSProperties, ReactNode } from "react";

import { Pagination } from "../ui/Pagination";
import { Skeleton } from "../ui/Skeleton";

export type SortDirection = "ascending" | "descending";

export interface DataTableSort {
  columnId: string;
  direction: SortDirection;
}

export interface DataTableColumn<Row> {
  align?: "left" | "center" | "right";
  header: ReactNode;
  id: string;
  render: (row: Row) => ReactNode;
  sortable?: boolean;
  width?: CSSProperties["width"];
}

export interface DataTablePagination {
  onPageChange: (page: number) => void;
  page: number;
  pageCount: number;
}

export interface DataTableProps<Row> {
  caption: string;
  className?: string;
  columns: DataTableColumn<Row>[];
  emptyMessage?: ReactNode;
  error?: string;
  getRowId: (row: Row) => string;
  getRowLabel?: (row: Row) => string;
  isLoading?: boolean;
  loadingRowCount?: number;
  onRowClick?: (row: Row) => void;
  onSelectionChange?: (selectedIds: Set<string>) => void;
  onSortChange?: (sort?: DataTableSort) => void;
  pagination?: DataTablePagination;
  rows: Row[];
  selectedRowIds?: ReadonlySet<string>;
  sort?: DataTableSort;
  visibleCaption?: boolean;
}

interface SelectionCheckboxProps {
  checked: boolean;
  indeterminate?: boolean;
  label: string;
  onChange: () => void;
}

function SelectionCheckbox({ checked, indeterminate = false, label, onChange }: SelectionCheckboxProps) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => { if (ref.current) ref.current.indeterminate = indeterminate; }, [indeterminate]);
  return <input aria-label={label} checked={checked} onChange={onChange} ref={ref} type="checkbox" />;
}

export function DataTable<Row>({
  caption,
  className = "",
  columns,
  emptyMessage = "No results found.",
  error,
  getRowId,
  getRowLabel,
  isLoading = false,
  loadingRowCount = 5,
  onRowClick,
  onSelectionChange,
  onSortChange,
  pagination,
  rows,
  selectedRowIds = new Set<string>(),
  sort,
  visibleCaption = false,
}: DataTableProps<Row>) {
  const selectable = Boolean(onSelectionChange);
  const rowIds = rows.map(getRowId);
  const selectedOnPage = rowIds.filter((id) => selectedRowIds.has(id));
  const allSelected = rowIds.length > 0 && selectedOnPage.length === rowIds.length;
  const someSelected = selectedOnPage.length > 0 && !allSelected;
  const updateSelection = (id: string) => {
    const next = new Set(selectedRowIds);
    if (next.has(id)) next.delete(id); else next.add(id);
    onSelectionChange?.(next);
  };
  const togglePage = () => {
    const next = new Set(selectedRowIds);
    if (allSelected) rowIds.forEach((id) => next.delete(id));
    else rowIds.forEach((id) => next.add(id));
    onSelectionChange?.(next);
  };
  const updateSort = (column: DataTableColumn<Row>) => {
    if (!column.sortable || !onSortChange) return;
    if (sort?.columnId !== column.id) onSortChange({ columnId: column.id, direction: "ascending" });
    else if (sort.direction === "ascending") onSortChange({ columnId: column.id, direction: "descending" });
    else onSortChange(undefined);
  };
  const columnCount = columns.length + (selectable ? 1 : 0);

  return (
    <section aria-busy={isLoading || undefined} aria-label={caption} className={`ui-data-table ${className}`.trim()}>
      {error ? <div className="ui-data-table__error" role="alert">{error}</div> : null}
      <div className="ui-data-table__scroll" tabIndex={0}>
        <table>
          <caption className={visibleCaption ? "ui-data-table__caption" : "ui-visually-hidden"}>{caption}</caption>
          <thead>
            <tr>
              {selectable ? <th className="ui-data-table__selection" scope="col">
                <SelectionCheckbox checked={allSelected} indeterminate={someSelected} label={allSelected ? "Deselect all rows on this page" : "Select all rows on this page"} onChange={togglePage} />
              </th> : null}
              {columns.map((column) => {
                const activeSort = sort?.columnId === column.id ? sort.direction : undefined;
                return <th aria-sort={activeSort ?? "none"} key={column.id} scope="col" style={{ textAlign: column.align, width: column.width }}>
                  {column.sortable && onSortChange ? <button className="ui-data-table__sort" onClick={() => updateSort(column)} type="button">
                    <span>{column.header}</span><span aria-hidden="true" className="ui-data-table__sort-icon">{activeSort === "ascending" ? "↑" : activeSort === "descending" ? "↓" : "↕"}</span>
                  </button> : column.header}
                </th>;
              })}
            </tr>
          </thead>
          <tbody>
            {isLoading ? Array.from({ length: loadingRowCount }, (_, rowIndex) => <tr aria-hidden="true" key={`loading-${rowIndex}`}>
              {selectable ? <td><Skeleton height="1rem" shape="rectangle" width="1rem" /></td> : null}
              {columns.map((column) => <td key={column.id}><Skeleton /></td>)}
            </tr>) : null}
            {!isLoading && !error && rows.length === 0 ? <tr><td className="ui-data-table__empty" colSpan={columnCount}>{emptyMessage}</td></tr> : null}
            {!isLoading && !error ? rows.map((row) => {
              const id = getRowId(row);
              const selected = selectedRowIds.has(id);
              return <tr
                aria-label={onRowClick ? getRowLabel?.(row) : undefined}
                aria-selected={selectable ? selected : undefined}
                className={onRowClick ? "ui-data-table__clickable-row" : undefined}
                key={id}
                onClick={onRowClick ? () => onRowClick(row) : undefined}
                onKeyDown={onRowClick ? (event) => {
                  if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onRowClick(row); }
                } : undefined}
                tabIndex={onRowClick ? 0 : undefined}
              >
                {selectable ? <td className="ui-data-table__selection" onClick={(event) => event.stopPropagation()}>
                  <SelectionCheckbox checked={selected} label={`${selected ? "Deselect" : "Select"} ${getRowLabel?.(row) ?? `row ${id}`}`} onChange={() => updateSelection(id)} />
                </td> : null}
                {columns.map((column) => <td key={column.id} style={{ textAlign: column.align }}>{column.render(row)}</td>)}
              </tr>;
            }) : null}
          </tbody>
        </table>
      </div>
      {pagination && !isLoading && !error ? <div className="ui-data-table__pagination"><Pagination {...pagination} /></div> : null}
    </section>
  );
}
