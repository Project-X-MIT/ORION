import { loadAppConfig } from "../../app/config";
import {
  API_ERROR_CODES,
  ApiClientError,
  cancelledError,
  networkError,
  timeoutError,
  type ApiErrorCode,
  type ApiFailureBody,
} from "./errors";

const SUPPORTED_API_VERSION = 1;

type ApiSuccessBody<T> = {
  api_version: number;
  request_id: string;
  data: T;
};

type Fetch = typeof fetch;

export type ApiClientOptions = {
  baseUrl: string;
  timeoutMs?: number;
  fetch?: Fetch;
  onUnauthenticated?: (error: ApiClientError) => void;
};

export type ApiRequestOptions = Omit<RequestInit, "body"> & {
  body?: BodyInit | object | null;
  timeoutMs?: number;
};

export type ApiClient = {
  request<T>(path: string, options?: ApiRequestOptions): Promise<T>;
  get<T>(path: string, options?: ApiRequestOptions): Promise<T>;
  post<T>(path: string, body?: ApiRequestOptions["body"], options?: ApiRequestOptions): Promise<T>;
  put<T>(path: string, body?: ApiRequestOptions["body"], options?: ApiRequestOptions): Promise<T>;
  patch<T>(path: string, body?: ApiRequestOptions["body"], options?: ApiRequestOptions): Promise<T>;
  delete<T>(path: string, options?: ApiRequestOptions): Promise<T>;
};

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isErrorCode(value: unknown): value is ApiErrorCode {
  return typeof value === "string" && API_ERROR_CODES.some((code) => code === value);
}

function isFailureBody(value: unknown): value is ApiFailureBody {
  if (!isObject(value) || !isObject(value.error)) return false;
  const details = value.error.details;
  return (
    typeof value.api_version === "number" &&
    typeof value.request_id === "string" &&
    isErrorCode(value.error.code) &&
    typeof value.error.message === "string" &&
    (details === undefined ||
      (isObject(details) && Object.values(details).every((detail) => typeof detail === "string")))
  );
}

function isSuccessBody<T>(value: unknown): value is ApiSuccessBody<T> {
  return (
    isObject(value) &&
    typeof value.api_version === "number" &&
    typeof value.request_id === "string" &&
    Object.prototype.hasOwnProperty.call(value, "data")
  );
}

function joinUrl(baseUrl: string, path: string): string {
  const normalizedBase = baseUrl.replace(/\/+$/, "");
  const normalizedPath = path.replace(/^\/+/, "");
  return normalizedPath ? `${normalizedBase}/${normalizedPath}` : normalizedBase;
}

function serializeBody(body: ApiRequestOptions["body"], headers: Headers): BodyInit | null | undefined {
  if (body === undefined || body === null || typeof body === "string" || body instanceof Blob ||
      body instanceof FormData || body instanceof URLSearchParams || body instanceof ArrayBuffer ||
      ArrayBuffer.isView(body)) {
    return body as BodyInit | null | undefined;
  }

  if (!headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  return JSON.stringify(body);
}

async function readBody(response: Response): Promise<unknown> {
  if (response.status === 204) return undefined;
  const text = await response.text();
  if (!text) return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

export function createApiClient(options: ApiClientOptions): ApiClient {
  const requestFetch = options.fetch ?? globalThis.fetch;
  const defaultTimeoutMs = options.timeoutMs ?? 15_000;

  async function request<T>(path: string, requestOptions: ApiRequestOptions = {}): Promise<T> {
    const { body, headers: requestHeaders, timeoutMs = defaultTimeoutMs, ...init } = requestOptions;
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
      throw new ApiClientError("Request timeout must be greater than zero", {
        kind: "protocol",
        status: 0,
      });
    }
    const headers = new Headers(requestHeaders);
    if (!headers.has("Accept")) headers.set("Accept", "application/json");

    const controller = new AbortController();
    const abortFromCaller = () => controller.abort(requestOptions.signal?.reason);
    if (requestOptions.signal?.aborted) {
      controller.abort(requestOptions.signal.reason);
    } else {
      requestOptions.signal?.addEventListener("abort", abortFromCaller, { once: true });
    }
    let didTimeout = false;
    const timeout = globalThis.setTimeout(() => {
      didTimeout = true;
      controller.abort();
    }, timeoutMs);

    const throwIfAborted = (cause?: unknown) => {
      if (requestOptions.signal?.aborted) throw cancelledError(cause);
      if (didTimeout || controller.signal.aborted) throw timeoutError(cause);
    };

    let response: Response;
    try {
      response = await requestFetch(joinUrl(options.baseUrl, path), {
        ...init,
        body: serializeBody(body, headers),
        credentials: "include",
        headers,
        signal: controller.signal,
      });
      throwIfAborted();
    } catch (cause) {
      throwIfAborted(cause);
      throw networkError(cause);
    } finally {
      globalThis.clearTimeout(timeout);
      requestOptions.signal?.removeEventListener("abort", abortFromCaller);
    }

    let responseBody: unknown;
    try {
      responseBody = await readBody(response);
      throwIfAborted();
    } catch (cause) {
      throwIfAborted(cause);
      throw networkError(cause);
    }
    if (!response.ok) {
      const failure = isFailureBody(responseBody) ? responseBody : undefined;
      const error = new ApiClientError(failure?.error.message ?? `Request failed (${response.status})`, {
        kind: "http",
        status: response.status,
        code: failure?.error.code,
        details: failure?.error.details,
        requestId: failure?.request_id ?? response.headers.get("x-request-id") ?? undefined,
      });
      if (error.isUnauthenticated) options.onUnauthenticated?.(error);
      throw error;
    }

    if (!isSuccessBody<T>(responseBody) || responseBody.api_version !== SUPPORTED_API_VERSION) {
      throw new ApiClientError("The API returned an unsupported response", {
        kind: "protocol",
        status: response.status,
        requestId: isObject(responseBody) && typeof responseBody.request_id === "string"
          ? responseBody.request_id
          : undefined,
      });
    }

    return responseBody.data;
  }

  return {
    request,
    get: (path, options) => request(path, { ...options, method: "GET" }),
    post: (path, body, options) => request(path, { ...options, method: "POST", body }),
    put: (path, body, options) => request(path, { ...options, method: "PUT", body }),
    patch: (path, body, options) => request(path, { ...options, method: "PATCH", body }),
    delete: (path, options) => request(path, { ...options, method: "DELETE" }),
  };
}

let defaultClient: ApiClient | undefined;

function getDefaultClient(): ApiClient {
  if (!defaultClient) {
    const config = loadAppConfig();
    defaultClient = createApiClient({
      baseUrl: config.apiBaseUrl,
      timeoutMs: config.requestTimeoutMs,
      onUnauthenticated: (error) => {
        window.dispatchEvent(new CustomEvent("orion:unauthenticated", { detail: error }));
      },
    });
  }
  return defaultClient;
}

export const apiClient: ApiClient = {
  request: (path, options) => getDefaultClient().request(path, options),
  get: (path, options) => getDefaultClient().get(path, options),
  post: (path, body, options) => getDefaultClient().post(path, body, options),
  put: (path, body, options) => getDefaultClient().put(path, body, options),
  patch: (path, body, options) => getDefaultClient().patch(path, body, options),
  delete: (path, options) => getDefaultClient().delete(path, options),
};
