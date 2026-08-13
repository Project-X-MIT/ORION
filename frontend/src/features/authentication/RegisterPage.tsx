import { useState, type FormEvent } from "react";

import { useAuth } from "../../providers/AuthProvider";

export function RegisterPage() {
  const { register, error } = useAuth();
  const [form, setForm] = useState({ email: "", username: "", password: "", display_name: "" });
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
    <main>
      <h1>Create your ORION account</h1>
      <form onSubmit={submit}>
        <label>Email <input value={form.email} onChange={(event) => setForm({ ...form, email: event.target.value })} type="email" required /></label>
        <label>Username <input value={form.username} onChange={(event) => setForm({ ...form, username: event.target.value })} required /></label>
        <label>Display name <input value={form.display_name} onChange={(event) => setForm({ ...form, display_name: event.target.value })} /></label>
        <label>Password <input value={form.password} onChange={(event) => setForm({ ...form, password: event.target.value })} type="password" minLength={12} required /></label>
        {error && <p role="alert">{error}</p>}
        <button disabled={busy} type="submit">{busy ? "Creating…" : "Create account"}</button>
      </form>
      <p><a href="/login">Already have an account?</a></p>
    </main>
  );
}
