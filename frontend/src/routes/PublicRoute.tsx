import type { PropsWithChildren } from "react";

import { useAuth } from "../providers/AuthProvider";

export function PublicRoute({ children }: PropsWithChildren) {
  const { status } = useAuth();
  if (status === "loading") return <p>Loading your session…</p>;
  if (status === "authenticated") {
    window.history.replaceState({}, "", "/");
    window.dispatchEvent(new PopStateEvent("popstate"));
    return null;
  }
  return <>{children}</>;
}
