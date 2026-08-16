import type { HTMLAttributes, ReactNode } from "react";

export interface LiveRegionProps extends HTMLAttributes<HTMLDivElement> {
  children?: ReactNode;
  politeness?: "polite" | "assertive" | "off";
}

export function LiveRegion({ children, politeness = "polite", ...props }: LiveRegionProps) {
  return <div aria-live={politeness} role={politeness === "off" ? undefined : "status"} {...props}>{children}</div>;
}
