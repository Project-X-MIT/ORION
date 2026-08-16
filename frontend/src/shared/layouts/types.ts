import type { ReactNode } from "react";

export interface LayoutNavItem {
  badge?: ReactNode;
  disabled?: boolean;
  exact?: boolean;
  href: string;
  icon?: ReactNode;
  label: string;
}
