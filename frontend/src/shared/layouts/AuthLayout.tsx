import type { ReactNode } from "react";

export interface AuthLayoutProps {
  aside?: ReactNode;
  brand?: ReactNode;
  brandHref?: string;
  children: ReactNode;
  className?: string;
  footer?: ReactNode;
  subtitle?: ReactNode;
  title?: ReactNode;
}

export function AuthLayout({ aside, brand = "ORION", brandHref = "/", children, className = "", footer, subtitle, title }: AuthLayoutProps) {
  return <div className={`ui-layout ui-auth-layout ${className}`.trim()}>
    <a className="ui-skip-link" href="#main-content">Skip to main content</a>
    <header className="ui-auth-layout__header"><a className="ui-layout__brand" href={brandHref}>{brand}</a></header>
    <div className="ui-auth-layout__shell">
      {aside ? <aside className="ui-auth-layout__aside" aria-label="About ORION">{aside}</aside> : null}
      <main className="ui-auth-layout__main" id="main-content" tabIndex={-1}>
        <section aria-labelledby={title ? "auth-layout-title" : undefined} className="ui-auth-layout__card">
          {title ? <header className="ui-auth-layout__intro"><h1 id="auth-layout-title">{title}</h1>{subtitle ? <div>{subtitle}</div> : null}</header> : null}
          {children}
          {footer ? <footer className="ui-auth-layout__footer">{footer}</footer> : null}
        </section>
      </main>
    </div>
  </div>;
}
