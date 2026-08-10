import type { HTMLAttributes, ReactNode } from "react";

export type BadgeVariant = "neutral" | "info" | "success" | "warning" | "danger";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  dot?: boolean;
  icon?: ReactNode;
  variant?: BadgeVariant;
}

export function Badge({ children, className = "", dot = false, icon, variant = "neutral", ...props }: BadgeProps) {
  return (
    <span className={`ui-badge ui-badge--${variant} ${className}`.trim()} {...props}>
      {dot ? <span aria-hidden="true" className="ui-badge__dot" /> : icon}
      {children}
    </span>
  );
}
