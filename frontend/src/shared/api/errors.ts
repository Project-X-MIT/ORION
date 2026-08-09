export const API_ERROR_CODES = [
  "INVALID_REQUEST",
  "VALIDATION_FAILED",
  "UNAUTHENTICATED",
  "FORBIDDEN",
  "NOT_FOUND",
  "CONFLICT",
  "RATE_LIMITED",
  "SERVICE_UNAVAILABLE",
  "INTERNAL",
] as const;

export type ApiErrorCode = (typeof API_ERROR_CODES)[number];

export type ApiFailureBody = {
  api_version: number;
  request_id: string;
  error: {
    code: ApiErrorCode;
    message: string;
    details?: Record<string, string>;
  };
};

export type ApiErrorKind = "cancelled" | "timeout" | "network" | "http" | "protocol";

type ApiClientErrorOptions = {
  kind: ApiErrorKind;
  status: number;
  code?: ApiErrorCode;
  requestId?: string;
  details?: Record<string, string>;
  cause?: unknown;
};

export class ApiClientError extends Error {
  readonly kind: ApiErrorKind;
  readonly status: number;
  readonly code?: ApiErrorCode;
  readonly requestId?: string;
  readonly details: Readonly<Record<string, string>>;

  constructor(message: string, options: ApiClientErrorOptions) {
    super(message, { cause: options.cause });
    this.name = "ApiClientError";
    this.kind = options.kind;
    this.status = options.status;
    this.code = options.code;
    this.requestId = options.requestId;
    this.details = Object.freeze({ ...options.details });
  }

  get isUnauthenticated(): boolean {
    return this.status === 401 || this.code === "UNAUTHENTICATED";
  }

  get isRetryable(): boolean {
    return this.kind === "timeout" || this.kind === "network" ||
      this.status === 429 || this.status >= 500;
  }
}

export function cancelledError(cause?: unknown): ApiClientError {
  return new ApiClientError("The request was cancelled", { kind: "cancelled", status: 0, cause });
}

export function timeoutError(cause?: unknown): ApiClientError {
  return new ApiClientError("The request timed out", { kind: "timeout", status: 0, cause });
}

export function networkError(cause?: unknown): ApiClientError {
  return new ApiClientError("The API could not be reached", { kind: "network", status: 0, cause });
}

export function isApiClientError(error: unknown): error is ApiClientError {
  return error instanceof ApiClientError;
}
