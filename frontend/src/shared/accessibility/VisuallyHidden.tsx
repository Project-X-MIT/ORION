import type { HTMLAttributes, ReactNode } from "react";

export interface VisuallyHiddenProps extends HTMLAttributes<HTMLSpanElement> {
  children?: ReactNode;
}

export function VisuallyHidden({ children, className = "", ...props }: VisuallyHiddenProps) {
  return <span className={`ui-visually-hidden ${className}`.trim()} {...props}>{children}</span>;
}
