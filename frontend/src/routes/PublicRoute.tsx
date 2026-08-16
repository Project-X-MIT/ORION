import { useEffect, type PropsWithChildren } from "react";

import { useAuth } from "../providers/AuthProvider";

const AUTH_PATHS = new Set(["/login", "/register"]);

export function getAuthenticatedRedirect(search: string): string {
  const returnTo = new URLSearchParams(search).get("returnTo");
  if (
    !returnTo ||
    !returnTo.startsWith("/") ||
    returnTo.startsWith("//") ||
    returnTo.includes("\\")
  ) return "/";

  let destination: URL;
  try {
    destination = new URL(returnTo, "https://orion.local");
  } catch {
    return "/";
  }

  if (destination.origin !== "https://orion.local" || AUTH_PATHS.has(destination.pathname)) return "/";
  return `${destination.pathname}${destination.search}${destination.hash}`;
}

function RedirectToApplication() {
  useEffect(() => {
    const destination = getAuthenticatedRedirect(window.location.search);
    window.history.replaceState({}, "", destination);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, []);

  return <p aria-live="polite">Redirecting to ORION…</p>;
}

export function PublicRoute({ children }: PropsWithChildren) {
  const { status } = useAuth();
  if (status === "loading") return <p>Loading your session…</p>;
  if (status === "authenticated") return <RedirectToApplication />;
  return <>{children}</>;
}
