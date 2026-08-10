import { useId, useState } from "react";
import type { ReactNode } from "react";

import type { LayoutNavItem } from "./types";

export interface PublicLayoutProps {
  actions?: ReactNode;
  brand?: ReactNode;
  brandHref?: string;
  children: ReactNode;
  className?: string;
  currentPath?: string;
  footer?: ReactNode;
  navItems?: LayoutNavItem[];
}

function isActive(item: LayoutNavItem, path: string) {
  return item.exact ? path === item.href : path === item.href || (item.href !== "/" && path.startsWith(`${item.href}/`));
}

export function PublicLayout({ actions, brand = "ORION", brandHref = "/", children, className = "", currentPath = typeof window === "undefined" ? "/" : window.location.pathname, footer, navItems = [] }: PublicLayoutProps) {
  const navigationId = useId();
  const [menuOpen, setMenuOpen] = useState(false);
  return <div className={`ui-layout ui-public-layout ${className}`.trim()}>
    <a className="ui-skip-link" href="#main-content">Skip to main content</a>
    <header className="ui-public-header">
      <div className="ui-layout__container ui-public-header__inner">
        <a className="ui-layout__brand" href={brandHref}>{brand}</a>
        {navItems.length ? <>
          <button aria-controls={navigationId} aria-expanded={menuOpen} aria-label="Toggle navigation" className="ui-layout__menu-button" onClick={() => setMenuOpen((open) => !open)} type="button"><span aria-hidden="true">☰</span></button>
          <nav aria-label="Primary navigation" className={`ui-public-nav${menuOpen ? " ui-public-nav--open" : ""}`} id={navigationId}>
            {navItems.map((item) => item.disabled ? <span aria-disabled="true" className="ui-layout__nav-link ui-layout__nav-link--disabled" key={item.href}>{item.label}</span> : <a aria-current={isActive(item, currentPath) ? "page" : undefined} className="ui-layout__nav-link" href={item.href} key={item.href} onClick={() => setMenuOpen(false)}>{item.label}{item.badge}</a>)}
          </nav>
        </> : null}
        {actions ? <div className="ui-public-header__actions">{actions}</div> : null}
      </div>
    </header>
    <main className="ui-layout__main" id="main-content" tabIndex={-1}>{children}</main>
    {footer ? <footer className="ui-public-footer"><div className="ui-layout__container">{footer}</div></footer> : null}
  </div>;
}

export type { LayoutNavItem } from "./types";
