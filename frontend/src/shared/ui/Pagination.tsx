import { Button } from "./Button";

export interface PaginationProps {
  ariaLabel?: string;
  busy?: boolean;
  className?: string;
  disabled?: boolean;
  hasNext?: boolean;
  hasPrevious?: boolean;
  label?: string;
  onNext?: () => void;
  onPageChange?: (page: number) => void;
  onPrevious?: () => void;
  page?: number;
  pageCount?: number;
  siblingCount?: number;
}

type PageToken = number | "ellipsis-start" | "ellipsis-end";

function pages(current: number, count: number, siblings: number): PageToken[] {
  if (count <= 7 + siblings * 2) return Array.from({ length: count }, (_, index) => index + 1);
  const start = Math.max(2, current - siblings);
  const end = Math.min(count - 1, current + siblings);
  return [1, ...(start > 2 ? ["ellipsis-start" as const] : []), ...Array.from({ length: end - start + 1 }, (_, index) => start + index), ...(end < count - 1 ? ["ellipsis-end" as const] : []), count];
}

export function Pagination({
  ariaLabel,
  busy = false,
  className = "",
  disabled = false,
  hasNext = false,
  hasPrevious = false,
  label,
  onNext,
  onPageChange,
  onPrevious,
  page,
  pageCount,
  siblingCount = 1,
}: PaginationProps) {
  const isSimple = pageCount === undefined || onPageChange === undefined;
  if (isSimple) {
    return <nav aria-label={label ?? ariaLabel ?? "Pagination"} className={`ui-pagination ${className}`.trim()}>
      <Button aria-label="Previous page" disabled={disabled || busy || !hasPrevious} onClick={onPrevious} size="sm" variant="ghost">Previous</Button>
      <Button aria-label="Next page" disabled={disabled || busy || !hasNext} onClick={onNext} size="sm" variant="ghost">Next</Button>
    </nav>;
  }

  const safeCount = Math.max(1, pageCount);
  const safePage = Math.min(Math.max(1, page ?? 1), safeCount);
  return <nav aria-label={ariaLabel ?? label ?? "Pagination"} className={`ui-pagination ${className}`.trim()}>
    <Button aria-label="Previous page" disabled={disabled || safePage === 1} onClick={() => onPageChange(safePage - 1)} size="sm" variant="ghost">Previous</Button>
    {pages(safePage, safeCount, siblingCount).map((token) => typeof token === "number" ? <Button aria-current={token === safePage ? "page" : undefined} aria-label={`Page ${token}`} disabled={disabled} key={token} onClick={() => onPageChange(token)} size="sm" variant={token === safePage ? "primary" : "ghost"}>{token}</Button> : <span aria-hidden="true" className="ui-pagination__ellipsis" key={token}>…</span>)}
    <Button aria-label="Next page" disabled={disabled || safePage === safeCount} onClick={() => onPageChange(safePage + 1)} size="sm" variant="ghost">Next</Button>
  </nav>;
}
