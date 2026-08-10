import type { ButtonHTMLAttributes, ReactNode } from "react";

import { Spinner } from "./Spinner";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  fullWidth?: boolean;
  isLoading?: boolean;
  loadingLabel?: string;
  size?: ButtonSize;
  startIcon?: ReactNode;
  endIcon?: ReactNode;
  variant?: ButtonVariant;
}

export function Button({
  children,
  className = "",
  disabled,
  endIcon,
  fullWidth = false,
  isLoading = false,
  loadingLabel = "Loading",
  size = "md",
  startIcon,
  type = "button",
  variant = "primary",
  ...props
}: ButtonProps) {
  return (
    <button
      className={`ui-button ui-button--${variant} ui-button--${size}${fullWidth ? " ui-button--full" : ""} ${className}`.trim()}
      data-loading={isLoading || undefined}
      disabled={disabled || isLoading}
      type={type}
      {...props}
    >
      {isLoading ? <Spinner decorative size="sm" /> : startIcon}
      <span role={isLoading ? "status" : undefined}>{isLoading ? loadingLabel : children}</span>
      {!isLoading && endIcon}
    </button>
  );
}
