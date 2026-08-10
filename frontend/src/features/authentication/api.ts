import { apiClient } from "../../shared/api/client";
export { ApiClientError as AuthApiError } from "../../shared/api/errors";
import type { AuthUser } from "./types";

export function getCurrentUser(): Promise<AuthUser> {
  return apiClient.get<{ user: AuthUser }>("/auth/me").then((data) => data.user);
}

export function register(input: {
  email: string;
  username: string;
  password: string;
  display_name?: string;
}): Promise<AuthUser> {
  return apiClient.post<{ user: AuthUser }>("/auth/register", input).then((data) => data.user);
}

export function login(input: { email: string; password: string }): Promise<AuthUser> {
  return apiClient.post<{ user: AuthUser }>("/auth/login", input).then((data) => data.user);
}

export async function logout(): Promise<void> {
  await apiClient.post<{ logged_out: boolean }>("/auth/logout");
}
