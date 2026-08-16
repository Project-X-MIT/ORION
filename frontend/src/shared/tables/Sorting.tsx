import { SelectField, type SelectOption } from "../forms/SelectField";

export interface SortingProps {
  id?: string;
  label?: string;
  value: string;
  options: SelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
}

/** Shared sort selector. Sort direction and server ordering remain feature-owned. */
export function Sorting({ disabled = false, id, label = "Sort by", onChange, options, value }: SortingProps) {
  return <SelectField disabled={disabled} id={id} label={label} onChange={(event) => onChange(event.currentTarget.value)} options={options} value={value} />;
}
