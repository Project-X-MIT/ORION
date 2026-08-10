import { forwardRef, useId } from "react";
import type { InputHTMLAttributes, ReactNode } from "react";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  error?: string;
  hint?: string;
  label?: string;
  startAdornment?: ReactNode;
  endAdornment?: ReactNode;
  success?: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className = "", endAdornment, error, hint, id, label, startAdornment, success, ...props },
  ref,
) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const message = error ?? success ?? hint;
  const messageId = message ? `${inputId}-message` : undefined;
  const status = error ? "error" : success ? "success" : "default";

  return (
    <div className={`ui-field ui-field--${status} ${className}`.trim()}>
      {label ? <label className="ui-field__label" htmlFor={inputId}>{label}</label> : null}
      <div className="ui-input-wrap">
        {startAdornment ? <span className="ui-input__adornment" aria-hidden="true">{startAdornment}</span> : null}
        <input
          aria-describedby={messageId}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          id={inputId}
          ref={ref}
          {...props}
        />
        {endAdornment ? <span className="ui-input__adornment" aria-hidden="true">{endAdornment}</span> : null}
      </div>
      {message ? <p className="ui-field__message" id={messageId} role={error ? "alert" : undefined}>{message}</p> : null}
    </div>
  );
});
