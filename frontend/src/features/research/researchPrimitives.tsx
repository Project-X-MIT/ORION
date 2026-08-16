/**
 * Research-owned compatibility primitives.
 *
 * SHAURYA-01/02 provide the canonical shared implementations. These small
 * adapters keep this feature type-safe and runnable before those exports land;
 * their public props intentionally mirror the shared component contracts.
 */
import {
  forwardRef,
  useEffect,
  useRef,
  type ButtonHTMLAttributes,
  type ChangeEvent,
  type FormHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type RefObject,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";

const visuallyHiddenStyle = {
  border: 0,
  clip: "rect(0 0 0 0)",
  height: "1px",
  margin: "-1px",
  overflow: "hidden",
  padding: 0,
  position: "absolute" as const,
  whiteSpace: "nowrap" as const,
  width: "1px",
};

export function LiveRegion({ children }: { children: ReactNode }) {
  return <div aria-live="polite" role="status">{children}</div>;
}

export function VisuallyHidden({ children }: { children: ReactNode }) {
  return <span style={visuallyHiddenStyle}>{children}</span>;
}

type FormProps = FormHTMLAttributes<HTMLFormElement> & {
  description?: ReactNode;
  descriptionId?: string;
};

export function Form({ description, descriptionId, children, ...props }: FormProps) {
  return (
    <form {...props} aria-describedby={descriptionId ?? props["aria-describedby"]}>
      {description && <p id={descriptionId}>{description}</p>}
      {children}
    </form>
  );
}

type FieldMessageProps = {
  id: string;
  description?: ReactNode;
  error?: string;
  count?: string;
};

function FieldMessages({ id, description, error, count }: FieldMessageProps) {
  return (
    <>
      {description && <p id={`${id}-description`}>{description}</p>}
      {error && <p id={`${id}-error`} role="alert">{error}</p>}
      {count && <p id={`${id}-count`}>{count}</p>}
    </>
  );
}

function describedBy(id: string, description?: ReactNode, error?: string, count?: string) {
  return [
    description ? `${id}-description` : undefined,
    error ? `${id}-error` : undefined,
    count ? `${id}-count` : undefined,
  ].filter(Boolean).join(" ") || undefined;
}

type TextFieldProps = {
  id: string;
  name?: string;
  label: string;
  value: string;
  onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => void;
  description?: ReactNode;
  error?: string;
  count?: string;
  maxLength?: number;
  required?: boolean;
  rows?: number;
  multiline?: boolean;
};

export function TextField({
  id,
  name,
  label,
  value,
  onChange,
  description,
  error,
  count,
  maxLength,
  required,
  rows,
  multiline,
}: TextFieldProps) {
  const ariaDescribedBy = describedBy(id, description, error, count);
  return (
    <div>
      <label htmlFor={id}>{label}</label>
      {multiline || rows ? (
        <textarea
          id={id}
          name={name}
          value={value}
          onChange={onChange}
          maxLength={maxLength}
          required={required}
          rows={rows}
          aria-describedby={ariaDescribedBy}
          aria-invalid={Boolean(error)}
        />
      ) : (
        <input
          id={id}
          name={name}
          value={value}
          onChange={onChange}
          maxLength={maxLength}
          required={required}
          aria-describedby={ariaDescribedBy}
          aria-invalid={Boolean(error)}
        />
      )}
      <FieldMessages id={id} description={description} error={error} count={count} />
    </div>
  );
}

type MarkdownEditorProps = Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "id" | "onChange" | "value"> & {
  id: string;
  label: string;
  value: string;
  onChange: (event: ChangeEvent<HTMLTextAreaElement>) => void;
  description?: ReactNode;
  error?: string;
  count?: string;
};

export function MarkdownEditor({
  id,
  label,
  value,
  onChange,
  description,
  error,
  count,
  ...props
}: MarkdownEditorProps) {
  return (
    <div>
      <label htmlFor={id}>{label}</label>
      <textarea
        {...props}
        id={id}
        value={value}
        onChange={onChange}
        aria-describedby={describedBy(id, description, error, count)}
        aria-invalid={Boolean(error)}
      />
      <FieldMessages id={id} description={description} error={error} count={count} />
    </div>
  );
}

type NumberFieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, "id" | "onChange" | "value"> & {
  id: string;
  label: string;
  value: string | number;
  onChange: (event: ChangeEvent<HTMLInputElement>) => void;
  description?: ReactNode;
};

export function NumberField({ id, label, value, onChange, description, ...props }: NumberFieldProps) {
  return (
    <div>
      <label htmlFor={id}>{label}</label>
      <input
        {...props}
        id={id}
        type="number"
        value={value}
        onChange={onChange}
        aria-describedby={description ? `${id}-description` : undefined}
      />
      {description && <p id={`${id}-description`}>{description}</p>}
    </div>
  );
}

type SelectFieldProps = Omit<SelectHTMLAttributes<HTMLSelectElement>, "id" | "onChange" | "value"> & {
  id: string;
  label: string;
  value: string;
  onChange: (event: ChangeEvent<HTMLSelectElement>) => void;
  options: Array<{ value: string; label: string }>;
};

export function SelectField({ id, label, value, onChange, options, ...props }: SelectFieldProps) {
  return (
    <div>
      <label htmlFor={id}>{label}</label>
      <select {...props} id={id} value={value} onChange={onChange}>
        {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
    </div>
  );
}

type AlertProps = HTMLAttributes<HTMLDivElement> & { title?: ReactNode };

export function Alert({ title, children, ...props }: AlertProps) {
  return (
    <div {...props} role="alert">
      {title && <strong>{title}</strong>}
      {children}
    </div>
  );
}

type BadgeProps = HTMLAttributes<HTMLSpanElement>;

export function Badge({ children, ...props }: BadgeProps) {
  return <span {...props}>{children}</span>;
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "default" | "primary";
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant: _variant, children, ...props },
  ref,
) {
  return <button {...props} ref={ref}>{children}</button>;
});

type CardProps = HTMLAttributes<HTMLElement> & {
  as?: "article" | "div" | "section";
};

export function Card({ as: Component = "div", ...props }: CardProps) {
  return <Component {...props} />;
}

type PaginationProps = {
  label: string;
  page: number;
  hasPrevious: boolean;
  hasNext: boolean;
  busy?: boolean;
  onPrevious: () => void;
  onNext: () => void;
};

export function Pagination({
  label,
  page,
  hasPrevious,
  hasNext,
  busy,
  onPrevious,
  onNext,
}: PaginationProps) {
  return (
    <nav aria-label={label} aria-busy={busy}>
      <span>Page {page}</span>{" "}
      <Button type="button" onClick={onPrevious} disabled={!hasPrevious || busy}>Previous</Button>{" "}
      <Button type="button" onClick={onNext} disabled={!hasNext || busy}>Next</Button>
    </nav>
  );
}

type ModalProps = {
  open: boolean;
  title: string;
  description?: ReactNode;
  onClose?: () => void;
  initialFocusRef?: RefObject<HTMLButtonElement | null>;
  children: ReactNode;
};

export function Modal({ open, title, description, onClose, initialFocusRef, children }: ModalProps) {
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  const titleId = "research-modal-title";
  const descriptionId = description ? "research-modal-description" : undefined;

  useEffect(() => {
    if (!open) {
      restoreFocusRef.current?.focus();
      return undefined;
    }
    restoreFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    initialFocusRef?.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose?.();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [initialFocusRef, onClose, open]);

  if (!open) return null;
  return (
    <div role="dialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={descriptionId}>
      <h2 id={titleId}>{title}</h2>
      {description && <p id={descriptionId}>{description}</p>}
      {children}
    </div>
  );
}
