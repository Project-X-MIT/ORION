import { useState, type FormEvent } from "react";

import { useAuth } from "../../providers/AuthProvider";

export function LoginPage() {
  const { login, error } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      await login({ email, password });
      window.history.replaceState({}, "", "/");
      window.dispatchEvent(new PopStateEvent("popstate"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main>
      <h1>Sign in to ORION</h1>
      <form onSubmit={submit}>
        <label>Email <input value={email} onChange={(event) => setEmail(event.target.value)} type="email" required /></label>
        <label>Password <input value={password} onChange={(event) => setPassword(event.target.value)} type="password" required /></label>
        {error && <p role="alert">{error}</p>}
        <button disabled={busy} type="submit">{busy ? "Signing in…" : "Sign in"}</button>
      </form>
      <p><a href="/register">Create an account</a></p>
    </main>
  );
}
