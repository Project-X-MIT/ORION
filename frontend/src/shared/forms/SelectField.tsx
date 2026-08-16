import { forwardRef, useId } from "react";
import type { ReactNode, SelectHTMLAttributes } from "react";

export type SelectOption = { value: string; label: ReactNode };

export interface SelectFieldProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "id" | "onChange" | "value"> {
  id?: string;
  error?: string;
  hint?: string;
  label?: string;
  onChange?: SelectHTMLAttributes<HTMLSelectElement>["onChange"];
  options: SelectOption[];
  success?: string;
  value?: string;
}

export const SelectField = forwardRef<HTMLSelectElement, SelectFieldProps>(function SelectField(
  { className = "", error, hint, id, label, onChange, options, success, value, ...props },
  ref,
) {
  const generatedId = useId();
  const selectId = id ?? generatedId;
  const message = error ?? success ?? hint;
  const messageId = message ? `${selectId}-message` : undefined;
  return (
    <div className={`ui-field ui-field--${error ? "error" : success ? "success" : "default"} ${className}`.trim()}>
      {label ? <label className="ui-field__label" htmlFor={selectId}>{label}</label> : null}
      <div className="ui-input-wrap">
        <select
          aria-describedby={messageId}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          id={selectId}
          onChange={onChange}
          ref={ref}
          value={value}
          {...props}
        >
          {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      </div>
      {message ? <p className="ui-field__message" id={messageId} role={error ? "alert" : undefined}>{message}</p> : null}
    </div>
  );
});
