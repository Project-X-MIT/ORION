import { useId, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";

export interface TabItem { id: string; label: ReactNode; content: ReactNode; disabled?: boolean }
export interface TabsProps { items: TabItem[]; value?: string; defaultValue?: string; onChange?: (id: string) => void; ariaLabel?: string; className?: string }

export function Tabs({ ariaLabel = "Tabs", className = "", defaultValue, items, onChange, value }: TabsProps) {
  const prefix = useId();
  const firstEnabled = items.find((item) => !item.disabled)?.id;
  const [internal, setInternal] = useState(defaultValue ?? firstEnabled);
  const selected = value ?? internal;
  const select = (id: string) => { if (value === undefined) setInternal(id); onChange?.(id); };
  const navigate = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const enabled = items.filter((item) => !item.disabled);
    const current = enabled.findIndex((item) => item.id === items[index]?.id);
    const next = event.key === "Home" ? 0 : event.key === "End" ? enabled.length - 1 : (current + (event.key === "ArrowRight" ? 1 : -1) + enabled.length) % enabled.length;
    const target = enabled[next];
    select(target.id);
    document.getElementById(`${prefix}-tab-${target.id}`)?.focus();
  };

  return <div className={`ui-tabs ${className}`.trim()}>
    <div aria-label={ariaLabel} className="ui-tabs__list" role="tablist">
      {items.map((item, index) => <button aria-controls={`${prefix}-panel-${item.id}`} aria-selected={selected === item.id} className="ui-tabs__tab" disabled={item.disabled} id={`${prefix}-tab-${item.id}`} key={item.id} onClick={() => select(item.id)} onKeyDown={(event) => navigate(event, index)} role="tab" tabIndex={selected === item.id ? 0 : -1} type="button">{item.label}</button>)}
    </div>
    {items.map((item) => <div aria-labelledby={`${prefix}-tab-${item.id}`} className="ui-tabs__panel" hidden={selected !== item.id} id={`${prefix}-panel-${item.id}`} key={item.id} role="tabpanel" tabIndex={0}>{item.content}</div>)}
  </div>;
}
