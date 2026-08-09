import type { ApiSuccess, AuthResponse, AuthUser } from "./types";

export class AuthApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "AuthApiError";
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  const body = (await response.json().catch(() => null)) as
    | ApiSuccess<T>
    | { error?: { message?: string } }
    | null;
  if (!response.ok) {
    throw new AuthApiError(
      body && "error" in body && body.error?.message
        ? body.error.message
        : "Authentication request failed",
      response.status,
    );
  }
  if (!body || !("data" in body)) {
    throw new AuthApiError("Authentication response was invalid", response.status);
  }
  return body.data;
}

export function getCurrentUser(): Promise<AuthUser> {
  return request<{ user: AuthUser }>("/api/v1/auth/me").then((data) => data.user);
}

export function register(input: {
  email: string;
  username: string;
  password: string;
  display_name?: string;
}): Promise<AuthUser> {
  return request<{ user: AuthUser }>("/api/v1/auth/register", {
    method: "POST",
    body: JSON.stringify(input),
  }).then((data) => data.user);
}

export function login(input: { email: string; password: string }): Promise<AuthUser> {
  return request<{ user: AuthUser }>("/api/v1/auth/login", {
    method: "POST",
    body: JSON.stringify(input),
  }).then((data) => data.user);
}

export async function logout(): Promise<void> {
  await request("/api/v1/auth/logout", { method: "POST" });
}
