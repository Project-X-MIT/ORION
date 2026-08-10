import type { CSSProperties, HTMLAttributes } from "react";

export interface SkeletonProps extends HTMLAttributes<HTMLSpanElement> {
  height?: CSSProperties["height"];
  shape?: "text" | "rectangle" | "circle";
  width?: CSSProperties["width"];
}

export function Skeleton({ className = "", height, shape = "text", style, width, ...props }: SkeletonProps) {
  return (
    <span
      aria-hidden="true"
      className={`ui-skeleton ui-skeleton--${shape} ${className}`.trim()}
      style={{ width, height, ...style }}
      {...props}
    />
  );
}
