import type { ReactNode } from "react";

export interface FiltersProps {
  children: ReactNode;
  label?: string;
  className?: string;
}

/** Groups related table filters under an announced fieldset. */
export function Filters({ children, className = "", label = "Filters" }: FiltersProps) {
  return <fieldset aria-label={label} className={`ui-table-filters ${className}`.trim()}><legend>{label}</legend>{children}</fieldset>;
}
