import { useState, type FormEvent } from "react";

import { useAuth } from "../../providers/AuthProvider";
import "./LoginPage.css";

export function RegisterPage() {
  const { register, error } = useAuth();
  const [form, setForm] = useState({ email: "", username: "", password: "", display_name: "" });
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      await register(form);
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

        <section aria-labelledby="register-title" className="login-page__card">
          <h1 id="register-title">Create Account</h1>
          <p className="login-page__subtitle">Enter your details below</p>
          <form className="login-page__form" onSubmit={submit}>
            <label className="login-page__field" htmlFor="register-email">
              <span>Email</span>
              <input id="register-email" value={form.email} onChange={(event) => setForm({ ...form, email: event.target.value })} type="email" autoComplete="email" placeholder="hello@orion.com" required />
            </label>
            <label className="login-page__field" htmlFor="register-username">
              <span>Username</span>
              <input id="register-username" value={form.username} onChange={(event) => setForm({ ...form, username: event.target.value })} autoComplete="username" placeholder="Choose a username" required />
            </label>
            <label className="login-page__field" htmlFor="register-display-name">
              <span>Display name</span>
              <input id="register-display-name" value={form.display_name} onChange={(event) => setForm({ ...form, display_name: event.target.value })} autoComplete="name" placeholder="Your name" />
            </label>
            <label className="login-page__field" htmlFor="register-password">
              <span>Password</span>
              <span className="login-page__input-wrap">
                <input id="register-password" value={form.password} onChange={(event) => setForm({ ...form, password: event.target.value })} type={showPassword ? "text" : "password"} autoComplete="new-password" placeholder="At least 12 characters" minLength={12} required />
                <button className="login-page__password-toggle" type="button" onClick={() => setShowPassword((visible) => !visible)} aria-label={showPassword ? "Hide password" : "Show password"}>
                  {showPassword ? "◉" : "◌"}
                </button>
              </span>
            </label>
            {error && <p className="login-page__error" role="alert">{error}</p>}
            <button className="login-page__submit" disabled={busy} type="submit">{busy ? "Creating…" : "Create account"}</button>
            <div className="login-page__divider"><span>or</span></div>
            <button className="login-page__google" type="button" disabled aria-label="Google sign-up is not available yet"><span aria-hidden="true">G</span> Sign up with Google</button>
          </form>
          <p className="login-page__signup">Already have an account? <a href="/login">Log in</a></p>
        </section>
      </div>
    </main>
  );
}
