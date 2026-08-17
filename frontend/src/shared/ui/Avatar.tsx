import { useState } from "react";
import type { HTMLAttributes } from "react";

export interface AvatarProps extends HTMLAttributes<HTMLSpanElement> {
  alt: string;
  fallback?: string;
  size?: "sm" | "md" | "lg" | "xl";
  src?: string;
  status?: "online" | "offline" | "busy";
}

function initials(value: string) {
  return value.trim().split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
}

export function Avatar({ alt, className = "", fallback, size = "md", src, status, ...props }: AvatarProps) {
  const [failedSrc, setFailedSrc] = useState<string>();
  const showImage = Boolean(src && src !== failedSrc);

  return (
    <span aria-label={alt} className={`ui-avatar ui-avatar--${size} ${className}`.trim()} role="img" {...props}>
      {showImage ? <img alt="" src={src} onError={() => setFailedSrc(src)} /> : <span aria-hidden="true">{fallback ?? initials(alt)}</span>}
      {status ? <span aria-label={status} className={`ui-avatar__status ui-avatar__status--${status}`} role="status" /> : null}
    </span>
  );
}
