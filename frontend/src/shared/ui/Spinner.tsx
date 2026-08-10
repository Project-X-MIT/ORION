import type { HTMLAttributes } from "react";

export type SpinnerSize = "sm" | "md" | "lg";

export interface SpinnerProps extends HTMLAttributes<HTMLSpanElement> {
  decorative?: boolean;
  label?: string;
  size?: SpinnerSize;
}

export function Spinner({ className = "", decorative = false, label = "Loading", size = "md", ...props }: SpinnerProps) {
  return (
    <span
      aria-hidden={decorative || undefined}
      aria-label={decorative ? undefined : label}
      className={`ui-spinner ui-spinner--${size} ${className}`.trim()}
      role={decorative ? undefined : "status"}
      {...props}
    />
  );
}
