import { useCallback, useEffect, useId, useRef } from "react";
import type { MouseEvent, ReactNode, RefObject } from "react";
import { createPortal } from "react-dom";

export interface ModalProps {
  children: ReactNode;
  className?: string;
  closeLabel?: string;
  closeOnBackdrop?: boolean;
  description?: ReactNode;
  footer?: ReactNode;
  initialFocusRef?: RefObject<HTMLElement | null>;
  isOpen?: boolean;
  onClose?: () => void;
  open?: boolean;
  title: ReactNode;
}

const focusable = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function Modal({
  children,
  className = "",
  closeLabel = "Close dialog",
  closeOnBackdrop = true,
  description,
  footer,
  initialFocusRef,
  isOpen,
  onClose,
  open,
  title,
}: ModalProps) {
  const visible = isOpen ?? open ?? false;
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const close = useCallback(() => closeRef.current?.(), []);
  const titleId = useId();
  const descriptionId = useId();
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!visible) return;
    const previous = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    (initialFocusRef?.current ?? panel?.querySelector<HTMLElement>("[data-autofocus]") ?? panel?.querySelector<HTMLElement>(focusable) ?? panel)?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
      if (event.key !== "Tab" || !panel) return;
      const items = [...panel.querySelectorAll<HTMLElement>(focusable)];
      if (items.length === 0) {
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = originalOverflow;
      previous?.focus();
    };
  }, [close, initialFocusRef, visible]);

  if (!visible) return null;
  const backdropClick = (event: MouseEvent<HTMLDivElement>) => {
    if (closeOnBackdrop && event.target === event.currentTarget) close();
  };

  return createPortal(
    <div className="ui-modal__backdrop" onMouseDown={backdropClick}>
      <div aria-describedby={description ? descriptionId : undefined} aria-labelledby={titleId} aria-modal="true" className={`ui-modal ${className}`.trim()} ref={panelRef} role="dialog" tabIndex={-1}>
        <header className="ui-modal__header">
          <div><h2 id={titleId}>{title}</h2>{description ? <div className="ui-modal__description" id={descriptionId}>{description}</div> : null}</div>
          <button aria-label={closeLabel} className="ui-modal__close" onClick={close} type="button">×</button>
        </header>
        <div className="ui-modal__body">{children}</div>
        {footer ? <footer className="ui-modal__footer">{footer}</footer> : null}
      </div>
    </div>,
    document.body,
  );
}

export { Modal as Dialog };
