import { forwardRef, useId } from "react";
import type { InputHTMLAttributes } from "react";

export interface NumberFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "onChange"> {
  error?: string;
  hint?: string;
  label?: string;
  onChange?: InputHTMLAttributes<HTMLInputElement>["onChange"];
  onValueChange?: (value: number | null) => void;
  success?: string;
}

function precision(value: number) {
  return (String(value).split(".")[1] ?? "").length;
}

export const NumberField = forwardRef<HTMLInputElement, NumberFieldProps>(function NumberField(
  { className = "", disabled, error, hint, id, label, max, min, onChange, onValueChange, readOnly, step = 1, success, value, defaultValue, ...props },
  ref,
) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const message = error ?? success ?? hint;
  const messageId = message ? `${inputId}-message` : undefined;
  const stepValue = step === "any" ? 1 : Number(step);
  const adjust = (direction: 1 | -1) => {
    const input = document.getElementById(inputId) as HTMLInputElement | null;
    if (!input) return;
    const fallback = direction > 0 ? Number(min ?? 0) : Number(max ?? 0);
    const current = input.value === "" ? fallback : input.valueAsNumber;
    let next = current + direction * stepValue;
    if (min !== undefined) next = Math.max(next, Number(min));
    if (max !== undefined) next = Math.min(next, Number(max));
    next = Number(next.toFixed(precision(stepValue)));
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    valueSetter?.call(input, String(next));
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.focus();
  };

  return (
    <div className={`ui-field ui-number-field${error ? " ui-field--error" : success ? " ui-field--success" : ""} ${className}`.trim()}>
      {label ? <label className="ui-field__label" htmlFor={inputId}>{label}</label> : null}
      <div className="ui-input-wrap">
        <input
          aria-describedby={messageId}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          defaultValue={defaultValue}
          disabled={disabled}
          id={inputId}
          max={max}
          min={min}
          onChange={(event) => {
            onChange?.(event);
            onValueChange?.(event.currentTarget.value === "" ? null : event.currentTarget.valueAsNumber);
          }}
          readOnly={readOnly}
          ref={ref}
          step={step}
          type="number"
          value={value}
          {...props}
        />
        <span className="ui-number-field__controls">
          <button aria-label={`Increase ${label ?? "value"}`} disabled={disabled || readOnly} onClick={() => adjust(1)} type="button">+</button>
          <button aria-label={`Decrease ${label ?? "value"}`} disabled={disabled || readOnly} onClick={() => adjust(-1)} type="button">−</button>
        </span>
      </div>
      {message ? <p className="ui-field__message" id={messageId} role={error ? "alert" : undefined}>{message}</p> : null}
    </div>
  );
});
