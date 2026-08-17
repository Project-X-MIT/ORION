import { useId } from "react";
import type { FormEventHandler, FormHTMLAttributes, ReactNode } from "react";

import { Button } from "../ui/Button";

export interface FormProps extends Omit<FormHTMLAttributes<HTMLFormElement>, "onSubmit"> {
  actions?: ReactNode;
  cancelLabel?: string;
  description?: ReactNode;
  descriptionId?: string;
  error?: string;
  isSubmitting?: boolean;
  onCancel?: () => void;
  onSubmit: FormEventHandler<HTMLFormElement>;
  submitLabel?: string;
  success?: string;
}

export function Form({
  actions,
  children,
  className = "",
  description,
  descriptionId,
  error,
  isSubmitting = false,
  onCancel,
  onSubmit,
  submitLabel = "Submit",
  success,
  ...props
}: FormProps) {
  const generatedDescriptionId = useId();
  const resolvedDescriptionId = description ? descriptionId ?? generatedDescriptionId : undefined;
  return (
    <form
      aria-busy={isSubmitting || undefined}
      aria-describedby={resolvedDescriptionId}
      className={`ui-form ${className}`.trim()}
      noValidate
      onSubmit={onSubmit}
      {...props}
    >
      {description ? <p className="ui-form__description" id={resolvedDescriptionId}>{description}</p> : null}
      <fieldset className="ui-form__fields" disabled={isSubmitting}>
        {children}
      </fieldset>
      {error ? <div className="ui-form__status ui-form__status--error" role="alert">{error}</div> : null}
      {success ? <div className="ui-form__status ui-form__status--success" role="status">{success}</div> : null}
      <div className="ui-form__actions">
        {actions ?? <>
          {onCancel ? <Button disabled={isSubmitting} onClick={onCancel} variant="secondary">Cancel</Button> : null}
          <Button isLoading={isSubmitting} loadingLabel="Submitting" type="submit">{submitLabel}</Button>
        </>}
      </div>
    </form>
  );
}
