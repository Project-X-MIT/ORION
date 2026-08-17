import type { HTMLAttributes, KeyboardEvent, ReactNode } from "react";

export interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "onClick" | "onKeyDown"> {
  as?: "article" | "div" | "section";
  footer?: ReactNode;
  header?: ReactNode;
  onActivate?: () => void;
  status?: "default" | "error" | "success";
}

export function Card({ as = "article", children, className = "", footer, header, onActivate, status = "default", ...props }: CardProps) {
  const Component = as;
  const activateFromKeyboard = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onActivate?.();
    }
  };
  return (
    <Component
      className={`ui-card ui-card--${status}${onActivate ? " ui-card--interactive" : ""} ${className}`.trim()}
      onClick={onActivate}
      onKeyDown={onActivate ? activateFromKeyboard : undefined}
      role={onActivate ? "button" : undefined}
      tabIndex={onActivate ? 0 : undefined}
      {...props}
    >
      {header ? <header className="ui-card__header">{header}</header> : null}
      <div className="ui-card__body">{children}</div>
      {footer ? <footer className="ui-card__footer">{footer}</footer> : null}
    </Component>
  );
}
