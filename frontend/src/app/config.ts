export type AppConfig = Readonly<{
  apiBaseUrl: string;
  requestTimeoutMs: number;
}>;

export type AppEnvironment = Readonly<Record<string, string | undefined>>;

const DEFAULT_API_BASE_URL = "/api/v1";
const DEFAULT_REQUEST_TIMEOUT_MS = 15_000;

function normalizeApiBaseUrl(value: string | undefined): string {
  const baseUrl = value?.trim() || DEFAULT_API_BASE_URL;

  if (!baseUrl.startsWith("/") && !URL.canParse(baseUrl)) {
    throw new Error("VITE_API_BASE_URL must be an absolute URL or a root-relative path");
  }

  return baseUrl.replace(/\/+$/, "");
}

function parseRequestTimeout(value: string | undefined): number {
  if (value === undefined || value.trim() === "") return DEFAULT_REQUEST_TIMEOUT_MS;

  const timeout = Number(value);
  if (!Number.isInteger(timeout) || timeout <= 0) {
    throw new Error("VITE_API_REQUEST_TIMEOUT_MS must be a positive integer");
  }

  return timeout;
}

export function createAppConfig(environment: AppEnvironment): AppConfig {
  return Object.freeze({
    apiBaseUrl: normalizeApiBaseUrl(environment.VITE_API_BASE_URL),
    requestTimeoutMs: parseRequestTimeout(environment.VITE_API_REQUEST_TIMEOUT_MS),
  });
}

export function loadAppConfig(): AppConfig {
  const environment = (import.meta as ImportMeta & { readonly env: AppEnvironment }).env;
  return createAppConfig(environment);
}
