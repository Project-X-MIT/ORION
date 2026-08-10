import { useEffect, useState } from "react";

import { LoginPage } from "../features/authentication/LoginPage";
import { RegisterPage } from "../features/authentication/RegisterPage";
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
  const { user, logout } = useAuth();
  if (path === "/login") return <PublicRoute><LoginPage /></PublicRoute>;
  if (path === "/register") return <PublicRoute><RegisterPage /></PublicRoute>;
  return <ProtectedRoute>
    <main>
      <h1>Welcome to ORION{user ? `, ${user.display_name ?? user.username}` : ""}</h1>
      <p>Your authenticated platform shell is ready.</p>
      <button type="button" onClick={() => void logout()}>Sign out</button>
    </main>
  </ProtectedRoute>;
}
