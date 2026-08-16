import { Input } from "../ui/Input";

export interface SearchProps {
  id?: string;
  label?: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
  onClear?: () => void;
  disabled?: boolean;
}

/** Shared labeled search control for table and list views. */
export function Search({
  disabled = false,
  id,
  label = "Search",
  onChange,
  onClear,
  placeholder,
  value,
}: SearchProps) {
  return (
    <div className="ui-table-search">
      <Input
        disabled={disabled}
        id={id}
        label={label}
        onChange={(event) => onChange(event.currentTarget.value)}
        placeholder={placeholder}
        type="search"
        value={value}
      />
      {value && onClear ? <button aria-label={`Clear ${label.toLowerCase()}`} className="ui-table-search__clear" disabled={disabled} onClick={onClear} type="button">Clear</button> : null}
    </div>
  );
}
