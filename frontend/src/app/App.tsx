import { useEffect, useState } from "react";

import { LoginPage } from "../features/authentication/LoginPage";
import { RegisterPage } from "../features/authentication/RegisterPage";
import { QuizPage } from "../features/quiz/QuizPage";
import { useAuth } from "../providers/AuthProvider";
import { ProtectedRoute } from "../routes/ProtectedRoute";
import { PublicRoute } from "../routes/PublicRoute";

function usePathname() {
  const [path, setPath] = useState(window.location.pathname);
  useEffect(() => {
    const update = () => setPath(window.location.pathname);
    window.addEventListener("popstate", update);
    return () => window.removeEventListener("popstate", update);
  }, []);
  return path;
}

export function App() {
  const path = usePathname();
  const { status, user, bootstrapError, logout, refresh } = useAuth();

  if (status === "loading") {
    return (
      <main aria-busy="true" aria-live="polite">
        <p>Loading ORION…</p>
      </main>
    );
  }

  if (bootstrapError) {
    return (
      <main role="alert">
        <h1>We could not load ORION</h1>
        <p>{bootstrapError}</p>
        <button type="button" onClick={() => void refresh()}>Try again</button>
      </main>
    );
  }

  if (path === "/login") return <PublicRoute><LoginPage /></PublicRoute>;
  if (path === "/register") return <PublicRoute><RegisterPage /></PublicRoute>;
  if (path === "/quiz") return <ProtectedRoute><QuizPage /></ProtectedRoute>;
  return <ProtectedRoute>
    <main>
      <h1>Welcome to ORION{user ? `, ${user.display_name ?? user.username}` : ""}</h1>
      <p>Your authenticated platform shell is ready.</p>
      <button type="button" onClick={() => void logout()}>Sign out</button>
    </main>
  </ProtectedRoute>;
}
