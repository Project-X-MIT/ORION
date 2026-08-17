import { forwardRef, useId } from "react";
import type { InputHTMLAttributes, ReactNode, Ref, TextareaHTMLAttributes } from "react";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  count?: string;
  description?: ReactNode;
  error?: string;
  hint?: string;
  label?: string;
  multiline?: boolean;
  rows?: number;
  startAdornment?: ReactNode;
  endAdornment?: ReactNode;
  success?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className = "", count, description, endAdornment, error, hint, id, label, multiline = false, rows, startAdornment, success, ...props },
  ref,
) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const message = error ?? success ?? hint;
  const descriptionId = description ? `${inputId}-description` : undefined;
  const messageId = message ? `${inputId}-message` : undefined;
  const countId = count ? `${inputId}-count` : undefined;
  const describedBy = [descriptionId, messageId, countId].filter(Boolean).join(" ") || undefined;
  const status = error ? "error" : success ? "success" : "default";

  return (
    <div className={`ui-field ui-field--${status} ${className}`.trim()}>
      {label ? <label className="ui-field__label" htmlFor={inputId}>{label}</label> : null}
      <div className="ui-input-wrap">
        {startAdornment ? <span className="ui-input__adornment" aria-hidden="true">{startAdornment}</span> : null}
        {multiline ? <textarea
          aria-describedby={describedBy}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          id={inputId}
          ref={ref as unknown as Ref<HTMLTextAreaElement>}
          rows={rows}
          {...(props as unknown as TextareaHTMLAttributes<HTMLTextAreaElement>)}
        /> : <input
          aria-describedby={describedBy}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          id={inputId}
          ref={ref}
          {...props}
        />}
        {endAdornment ? <span className="ui-input__adornment" aria-hidden="true">{endAdornment}</span> : null}
      </div>
      {description ? <p className="ui-field__message" id={descriptionId}>{description}</p> : null}
      {message ? <p className="ui-field__message" id={messageId} role={error ? "alert" : undefined}>{message}</p> : null}
      {count ? <p className="ui-field__message" id={countId}>{count}</p> : null}
    </div>
  );
});
