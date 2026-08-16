import { forwardRef, useId } from "react";
import type { ReactNode, TextareaHTMLAttributes } from "react";

export interface MarkdownEditorProps extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "id" | "onChange" | "value"> {
  count?: string;
  description?: ReactNode;
  error?: string;
  hint?: string;
  label?: string;
  onChange?: TextareaHTMLAttributes<HTMLTextAreaElement>["onChange"];
  success?: string;
  value?: string;
  id?: string;
}

export const MarkdownEditor = forwardRef<HTMLTextAreaElement, MarkdownEditorProps>(function MarkdownEditor(
  { className = "", count, description, error, hint, id, label, onChange, success, value, ...props },
  ref,
) {
  const generatedId = useId();
  const editorId = id ?? generatedId;
  const message = error ?? success ?? hint;
  const descriptionId = description ? `${editorId}-description` : undefined;
  const messageId = message ? `${editorId}-message` : undefined;
  const countId = count ? `${editorId}-count` : undefined;
  const describedBy = [descriptionId, messageId, countId].filter(Boolean).join(" ") || undefined;
  return (
    <div className={`ui-field ui-field--${error ? "error" : success ? "success" : "default"} ${className}`.trim()}>
      {label ? <label className="ui-field__label" htmlFor={editorId}>{label}</label> : null}
      <div className="ui-input-wrap">
        <textarea
          aria-describedby={describedBy}
          aria-invalid={error ? true : undefined}
          className="ui-input"
          id={editorId}
          onChange={onChange}
          ref={ref}
          value={value}
          {...props}
        />
      </div>
      {description ? <p className="ui-field__message" id={descriptionId}>{description}</p> : null}
      {message ? <p className="ui-field__message" id={messageId} role={error ? "alert" : undefined}>{message}</p> : null}
      {count ? <p className="ui-field__message" id={countId}>{count}</p> : null}
    </div>
  );
});
