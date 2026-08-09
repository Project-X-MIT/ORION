export type AuthUser = {
  id: string;
  email: string;
  username: string;
  display_name: string | null;
  status: string;
  role: "user" | "reviewer" | "admin";
};

export type ApiSuccess<T> = {
  api_version: number;
  request_id: string;
  data: T;
};

export type AuthResponse = ApiSuccess<{ user: AuthUser }>;

export type AuthStatus = "loading" | "authenticated" | "signed_out";
