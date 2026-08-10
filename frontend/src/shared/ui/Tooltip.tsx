import { cloneElement, isValidElement, useId, useState } from "react";
import type { FocusEvent, MouseEvent, ReactElement, ReactNode } from "react";

export interface TooltipProps { children: ReactElement; content: ReactNode; placement?: "top" | "right" | "bottom" | "left"; className?: string }

export function Tooltip({ children, className = "", content, placement = "top" }: TooltipProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  if (!isValidElement(children)) return null;
  const child = children as ReactElement<Record<string, unknown>>;
  const nativeFocusable = typeof child.type === "string" && ["a", "button", "input", "select", "textarea", "summary"].includes(child.type);
  const trigger = cloneElement(child, {
    "aria-describedby": open ? id : undefined,
    onBlur: (event: FocusEvent) => { (child.props.onBlur as ((event: FocusEvent) => void) | undefined)?.(event); setOpen(false); },
    onFocus: (event: FocusEvent) => { (child.props.onFocus as ((event: FocusEvent) => void) | undefined)?.(event); setOpen(true); },
    onMouseEnter: (event: MouseEvent) => { (child.props.onMouseEnter as ((event: MouseEvent) => void) | undefined)?.(event); setOpen(true); },
    onMouseLeave: (event: MouseEvent) => { (child.props.onMouseLeave as ((event: MouseEvent) => void) | undefined)?.(event); setOpen(false); },
    tabIndex: child.props.tabIndex ?? (nativeFocusable ? undefined : 0),
  });
  return <span className={`ui-tooltip ${className}`.trim()}>{trigger}{open ? <span className={`ui-tooltip__content ui-tooltip__content--${placement}`} id={id} role="tooltip">{content}</span> : null}</span>;
}
