import { useId, useState } from "react";
import type { ReactNode } from "react";

import { Avatar } from "../ui/Avatar";
import type { LayoutNavItem } from "./types";

export interface DashboardUser {
  avatarUrl?: string;
  name: string;
  secondaryText?: string;
}

export interface DashboardLayoutProps {
  brand?: ReactNode;
  brandHref?: string;
  children: ReactNode;
  className?: string;
  currentPath?: string;
  headerActions?: ReactNode;
  navItems: LayoutNavItem[];
  onSignOut?: () => void;
  pageTitle?: ReactNode;
  sidebarFooter?: ReactNode;
  user?: DashboardUser;
}

function isActive(item: LayoutNavItem, path: string) {
  return item.exact ? path === item.href : path === item.href || (item.href !== "/" && path.startsWith(`${item.href}/`));
}

export function DashboardLayout({ brand = "ORION", brandHref = "/", children, className = "", currentPath = typeof window === "undefined" ? "/" : window.location.pathname, headerActions, navItems, onSignOut, pageTitle, sidebarFooter, user }: DashboardLayoutProps) {
  const sidebarId = useId();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const closeSidebar = () => setSidebarOpen(false);
  return <div className={`ui-layout ui-dashboard-layout ${className}`.trim()}>
    <a className="ui-skip-link" href="#main-content">Skip to main content</a>
    {sidebarOpen ? <button aria-label="Close navigation" className="ui-dashboard-layout__overlay" onClick={closeSidebar} type="button" /> : null}
    <aside aria-label="Dashboard navigation" className={`ui-dashboard-sidebar${sidebarOpen ? " ui-dashboard-sidebar--open" : ""}`} id={sidebarId}>
      <div className="ui-dashboard-sidebar__brand"><a className="ui-layout__brand" href={brandHref}>{brand}</a><button aria-label="Close navigation" className="ui-dashboard-sidebar__close" onClick={closeSidebar} type="button">×</button></div>
      <nav aria-label="Dashboard">
        {navItems.map((item) => item.disabled ? <span aria-disabled="true" className="ui-dashboard-nav__link ui-layout__nav-link--disabled" key={item.href}>{item.icon}<span>{item.label}</span>{item.badge}</span> : <a aria-current={isActive(item, currentPath) ? "page" : undefined} className="ui-dashboard-nav__link" href={item.href} key={item.href} onClick={closeSidebar}>{item.icon ? <span aria-hidden="true" className="ui-dashboard-nav__icon">{item.icon}</span> : null}<span>{item.label}</span>{item.badge ? <span className="ui-dashboard-nav__badge">{item.badge}</span> : null}</a>)}
      </nav>
      {(user || sidebarFooter || onSignOut) ? <div className="ui-dashboard-sidebar__footer">
        {user ? <div className="ui-dashboard-user"><Avatar alt={user.name} size="sm" src={user.avatarUrl} /><span><strong>{user.name}</strong>{user.secondaryText ? <small>{user.secondaryText}</small> : null}</span></div> : null}
        {onSignOut ? <button className="ui-dashboard-signout" onClick={onSignOut} type="button">Sign out</button> : null}
        {sidebarFooter}
      </div> : null}
    </aside>
    <div className="ui-dashboard-content">
      <header className="ui-dashboard-header">
        <button aria-controls={sidebarId} aria-expanded={sidebarOpen} aria-label="Open navigation" className="ui-layout__menu-button" onClick={() => setSidebarOpen(true)} type="button"><span aria-hidden="true">☰</span></button>
        {pageTitle ? <div className="ui-dashboard-header__title">{pageTitle}</div> : null}
        {headerActions ? <div className="ui-dashboard-header__actions">{headerActions}</div> : null}
      </header>
      <main className="ui-dashboard-main" id="main-content" tabIndex={-1}>{children}</main>
    </div>
  </div>;
}

export type { LayoutNavItem } from "./types";
