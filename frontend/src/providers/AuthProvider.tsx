import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type PropsWithChildren,
} from "react";

import * as authApi from "../features/authentication/api";
import { AuthApiError } from "../features/authentication/api";
import type { AuthStatus, AuthUser } from "../features/authentication/types";

type AuthContextValue = {
  status: AuthStatus;
  user: AuthUser | null;
  error: string | null;
  login: (input: { email: string; password: string }) => Promise<AuthUser>;
  register: (input: {
    email: string;
    username: string;
    password: string;
    display_name?: string;
  }) => Promise<AuthUser>;
  logout: () => Promise<void>;
  refresh: () => Promise<AuthUser | null>;
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: PropsWithChildren) {
  const [status, setStatus] = useState<AuthStatus>("loading");
  const [user, setUser] = useState<AuthUser | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const current = await authApi.getCurrentUser();
      setUser(current);
      setStatus("authenticated");
      setError(null);
      return current;
    } catch (requestError) {
      setUser(null);
      setStatus("signed_out");
      if (requestError instanceof AuthApiError && requestError.status === 401) {
        setError(null);
      } else {
        setError(
          requestError instanceof Error ? requestError.message : "Could not load your session",
        );
      }
      return null;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const handleUnauthenticated = () => {
      setUser(null);
      setStatus("signed_out");
      setError(null);
    };
    window.addEventListener("orion:unauthenticated", handleUnauthenticated);
    return () => window.removeEventListener("orion:unauthenticated", handleUnauthenticated);
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      user,
      error,
      login: async (input) => {
        try {
          const next = await authApi.login(input);
          setUser(next);
          setStatus("authenticated");
          setError(null);
          return next;
        } catch (requestError) {
          const message = requestError instanceof Error ? requestError.message : "Login failed";
          setError(message);
          throw requestError;
        }
      },
      register: async (input) => {
        try {
          const next = await authApi.register(input);
          setUser(next);
          setStatus("authenticated");
          setError(null);
          return next;
        } catch (requestError) {
          const message = requestError instanceof Error ? requestError.message : "Registration failed";
          setError(message);
          throw requestError;
        }
      },
      logout: async () => {
        try {
          await authApi.logout();
          setUser(null);
          setStatus("signed_out");
          setError(null);
        } catch (requestError) {
          const message = requestError instanceof Error ? requestError.message : "Logout failed";
          setError(message);
          throw requestError;
        }
      },
      refresh,
    }),
    [error, refresh, status, user],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used inside AuthProvider");
  }
  return context;
}
