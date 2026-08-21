import { useEffect, useState } from "react";

import { LoginPage } from "../features/authentication/LoginPage";
import { RegisterPage } from "../features/authentication/RegisterPage";
import { DiscordConnect } from "../features/discord";
import { LandingPage } from "../features/landing/LandingPage";
import { LeaderboardPage } from "../features/leaderboard/LeaderboardPage";
import { LearningPage, LessonPage } from "../features/learning";
import { NewsPage } from "../features/news/NewsPage";
import { QuizPage } from "../features/quiz/QuizPage";
import { ResearchPage } from "../features/research/ResearchPage";
import { ProfilePage } from "../features/profile/ProfilePage";
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

  if (path === "/" && status !== "authenticated") return <LandingPage />;

  if (path === "/login") return <PublicRoute><LoginPage /></PublicRoute>;
  if (path === "/register") return <PublicRoute><RegisterPage /></PublicRoute>;

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

  if (path === "/leaderboard") return <LeaderboardPage />;
  if (path === "/quiz") return <ProtectedRoute><QuizPage /></ProtectedRoute>;
  if (path === "/learning") return <ProtectedRoute><LearningPage /></ProtectedRoute>;
  if (path.startsWith("/learning/lessons/")) return <ProtectedRoute><LessonPage /></ProtectedRoute>;
  if (path === "/news") return <NewsPage />;
  if (path === "/research" || path.startsWith("/research/")) return <ResearchPage />;
  if (path === "/discord") return <DiscordConnect />;
  if (path === "/profile" || path.startsWith("/profiles/")) return <ProtectedRoute><ProfilePage userId={path.startsWith("/profiles/") ? path.slice("/profiles/".length) : user?.id} /></ProtectedRoute>;
  return <ProtectedRoute>
    <main>
      <h1>Welcome to ORION{user ? `, ${user.display_name ?? user.username}` : ""}</h1>
      <p>Your authenticated platform shell is ready.</p>
      <button type="button" onClick={() => void logout()}>Sign out</button>
    </main>
  </ProtectedRoute>;
}
