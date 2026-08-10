import { useId, useRef, useState } from "react";
import type { ChangeEvent, DragEvent, InputHTMLAttributes } from "react";

export interface FileUploaderProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value" | "onChange"> {
  files?: File[];
  helperText?: string;
  label?: string;
  maxSize?: number;
  onFilesChange?: (files: File[]) => void;
  onReject?: (files: File[], reason: "type" | "size") => void;
}

function accepted(file: File, accept?: string) {
  if (!accept) return true;
  return accept.split(",").map((rule) => rule.trim().toLowerCase()).some((rule) => {
    if (rule.startsWith(".")) return file.name.toLowerCase().endsWith(rule);
    if (rule.endsWith("/*")) return file.type.startsWith(rule.slice(0, -1));
    return file.type === rule;
  });
}

export function FileUploader({
  accept,
  className = "",
  disabled,
  files,
  helperText,
  id,
  label = "Upload files",
  maxSize,
  multiple = false,
  onFilesChange,
  onReject,
  required,
  ...props
}: FileUploaderProps) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const inputRef = useRef<HTMLInputElement>(null);
  const [internalFiles, setInternalFiles] = useState<File[]>([]);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<string>();
  const selected = files ?? internalFiles;

  const update = (next: File[]) => {
    if (files === undefined) setInternalFiles(next);
    onFilesChange?.(next);
  };
  const add = (incoming: File[]) => {
    const wrongType = incoming.filter((file) => !accepted(file, accept));
    const tooLarge = incoming.filter((file) => maxSize !== undefined && file.size > maxSize);
    if (wrongType.length) onReject?.(wrongType, "type");
    if (tooLarge.length) onReject?.(tooLarge, "size");
    const valid = incoming.filter((file) => accepted(file, accept) && (maxSize === undefined || file.size <= maxSize));
    if (wrongType.length || tooLarge.length) setError(`${wrongType.length + tooLarge.length} file${wrongType.length + tooLarge.length === 1 ? " was" : "s were"} rejected.`);
    else setError(undefined);
    if (valid.length) update(multiple ? [...selected, ...valid] : valid.slice(0, 1));
  };
  const onInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    add([...event.currentTarget.files ?? []]);
    event.currentTarget.value = "";
  };
  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(false);
    if (!disabled) add([...event.dataTransfer.files]);
  };

  return (
    <div className={`ui-file-field ${className}`.trim()}>
      <label className="ui-field__label" htmlFor={inputId}>{label}{required ? " *" : ""}</label>
      <div
        className={`ui-file-dropzone${dragging ? " ui-file-dropzone--active" : ""}${disabled ? " ui-file-dropzone--disabled" : ""}`}
        onDragEnter={(event) => { event.preventDefault(); if (!disabled) setDragging(true); }}
        onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragging(false); }}
        onDragOver={(event) => event.preventDefault()}
        onDrop={onDrop}
      >
        <input accept={accept} className="ui-file-input" disabled={disabled} id={inputId} multiple={multiple} onChange={onInputChange} ref={inputRef} required={required && selected.length === 0} type="file" {...props} />
        <p><strong>Choose {multiple ? "files" : "a file"}</strong> or drag and drop</p>
        {helperText ? <p className="ui-file-field__help">{helperText}</p> : null}
      </div>
      {error ? <p className="ui-field__message ui-file-field__error" role="alert">{error}</p> : null}
      {selected.length ? <ul aria-label="Selected files" className="ui-file-list">
        {selected.map((file, index) => <li key={`${file.name}-${file.size}-${file.lastModified}`}>
          <span><strong>{file.name}</strong><small>{Math.max(1, Math.ceil(file.size / 1024))} KB</small></span>
          <button aria-label={`Remove ${file.name}`} disabled={disabled} onClick={() => update(selected.filter((_, itemIndex) => itemIndex !== index))} type="button">Remove</button>
        </li>)}
      </ul> : null}
    </div>
  );
}
