import { useState, type FormEvent } from "react";

import { useAuth } from "../../providers/AuthProvider";
import "./LoginPage.css";

export function LoginPage() {
  const { login, error } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
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
    <main className="login-page">
      <div className="login-page__shell">
        <section aria-label="ORION signal preview" className="login-page__visual">
          <img className="login-page__space-art" src="/login-space-reference.png" alt="" />
        </section>

        <section aria-labelledby="login-title" className="login-page__card">
          <h1 id="login-title">Welcome Back!</h1>
          <p className="login-page__subtitle">Enter your details below</p>
          <form className="login-page__form" onSubmit={submit}>
            <label className="login-page__field" htmlFor="login-email">
              <span>Email</span>
              <input id="login-email" value={email} onChange={(event) => setEmail(event.target.value)} type="email" autoComplete="email" placeholder="hello@orion.com" required />
            </label>
            <label className="login-page__field" htmlFor="login-password">
              <span>Password</span>
              <span className="login-page__input-wrap">
                <input id="login-password" value={password} onChange={(event) => setPassword(event.target.value)} type={showPassword ? "text" : "password"} autoComplete="current-password" placeholder="Enter your password" required />
                <button className="login-page__password-toggle" type="button" onClick={() => setShowPassword((visible) => !visible)} aria-label={showPassword ? "Hide password" : "Show password"}>
                  {showPassword ? "◉" : "◌"}
                </button>
              </span>
            </label>
            <div className="login-page__options">
              <label><input type="checkbox" defaultChecked /> <span>Remember me</span></label>
              <a href="/login#forgot-password">Forgot password?</a>
            </div>
            {error && <p className="login-page__error" role="alert">{error}</p>}
            <button className="login-page__submit" disabled={busy} type="submit">{busy ? "Logging in…" : "Log in"}</button>
            <div className="login-page__divider"><span>or</span></div>
            <button className="login-page__google" type="button" disabled aria-label="Google sign-in is not available yet"><span aria-hidden="true">G</span> Log in with Google</button>
          </form>
          <p className="login-page__signup">Don&apos;t have an account? <a href="/register">Sign Up</a></p>
        </section>
      </div>
    </main>
  );
}
