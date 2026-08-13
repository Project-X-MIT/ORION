import { useEffect, type PropsWithChildren } from "react";

import { useAuth } from "../providers/AuthProvider";

export function getLoginRedirect(location: Pick<Location, "pathname" | "search" | "hash">): string {
  const returnTo = `${location.pathname}${location.search}${location.hash}`;
  const parameters = new URLSearchParams({ returnTo });
  return `/login?${parameters.toString()}`;
}

function RedirectToLogin() {
  useEffect(() => {
    const destination = getLoginRedirect(window.location);
    window.history.replaceState({}, "", destination);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, []);

  return <p aria-live="polite">Redirecting to sign in…</p>;
}

export function ProtectedRoute({ children }: PropsWithChildren) {
  const { status } = useAuth();
  if (status === "loading") return <p>Loading your session…</p>;
  if (status !== "authenticated") return <RedirectToLogin />;
  return <>{children}</>;
}
