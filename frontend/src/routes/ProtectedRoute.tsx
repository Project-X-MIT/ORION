import type { PropsWithChildren } from "react";

import { useAuth } from "../providers/AuthProvider";

export function ProtectedRoute({ children }: PropsWithChildren) {
  const { status } = useAuth();
  if (status === "loading") return <p>Loading your session…</p>;
  if (status !== "authenticated") {
    window.history.replaceState({}, "", "/login");
    window.dispatchEvent(new PopStateEvent("popstate"));
    return null;
  }
  return <>{children}</>;
}
