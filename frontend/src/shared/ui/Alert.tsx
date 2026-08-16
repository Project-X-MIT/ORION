import type { HTMLAttributes, ReactNode } from "react";

export interface AlertProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  children?: ReactNode;
  title?: ReactNode;
  variant?: "info" | "success" | "warning" | "danger";
}

export function Alert({ children, className = "", title, variant = "info", ...props }: AlertProps) {
  return (
    <div
      aria-live={variant === "danger" || variant === "warning" ? "assertive" : "polite"}
      className={`ui-alert ui-alert--${variant} ${className}`.trim()}
      role="alert"
      {...props}
    >
      {title ? <strong className="ui-alert__title">{title}</strong> : null}
      {children}
    </div>
  );
}
