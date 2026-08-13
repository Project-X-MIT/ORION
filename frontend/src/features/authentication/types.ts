// Contract-derived from `orion-api::routes::auth::AuthUserResponse`.
export type AuthUser = {
  id: string;
  email: string;
  username: string;
  display_name: string | null;
  status: string;
  role: "user" | "reviewer" | "admin";
};

export type AuthStatus = "loading" | "authenticated" | "signed_out";
