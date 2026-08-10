# Frontend feature-module conventions

This guide applies to code under `frontend/src/features`. Feature owners own
their feature modules; Shaurya owns the application providers, route guards,
and shared API behavior. Div owns package and lockfiles and the application
router registry.

## Module shape

Keep feature behavior within its feature directory:

```text
features/<feature>/
  api.ts             HTTP operations only
  types.ts           contract-derived request and response types
  hooks.ts           UI-facing state and orchestration
  <Name>Page.tsx     route-level UI
  *.test.ts[x]       colocated tests
```

Small modules do not need every file. Do not move feature-specific business
rules into `shared`, providers, layouts, or the application router.

## API access

All HTTP operations use the exported `apiClient` from
`src/shared/api/client.ts`. Do not call `fetch` directly and do not create an
Axios instance or another API client.

```ts
import { apiClient } from "../../shared/api/client";
import type { Profile } from "./types";

export function getProfile(userId: string, signal?: AbortSignal): Promise<Profile> {
  return apiClient
    .get<{ profile: Profile }>(`/profiles/${encodeURIComponent(userId)}`, { signal })
    .then(({ profile }) => profile);
}
```

- Pass paths relative to the configured `/api/v1` base.
- Pass plain objects for JSON request bodies. The client sets JSON headers.
- Do not set authentication headers or read session cookies. The API uses an
  HTTP-only session cookie, and the client always sends credentials.
- Do not parse `ApiSuccess` or `ApiFailure` envelopes in feature modules. The
  client validates and unwraps them.
- Accept an `AbortSignal` for operations used by queries, searches, previews,
  or replaceable user actions, and forward it to the client.
- Keep request and response types derived from the shared API contract. Do not
  redefine a backend contract to make a component more convenient; map it to a
  separate view model when needed.

## Errors, cancellation, and retries

Feature code narrows unknown failures with `isApiClientError` from
`src/shared/api/errors.ts`.

```ts
import { isApiClientError } from "../../shared/api/errors";

try {
  await saveResearch(input, signal);
} catch (error) {
  if (isApiClientError(error) && error.kind === "cancelled") return;
  throw error;
}
```

The shared error kinds have consistent meanings:

| Kind | Meaning | Retry automatically? |
| --- | --- | --- |
| `cancelled` | The caller no longer needs the request | No |
| `timeout` | The configured deadline elapsed | Yes, when the operation is safe |
| `network` | The API or response stream could not be reached | Yes, when safe |
| `http` | The server returned a non-success status | Use `isRetryable` |
| `protocol` | Configuration or the versioned envelope is invalid | No |

Use stable `error.code` values for behavior and `error.details` for field-level
validation. Human-readable server messages may be displayed but must not drive
control flow. Include `error.requestId` in support diagnostics; never log
credentials, cookies, request bodies containing personal data, or report
contents.

An unauthenticated response updates the central authentication provider.
Feature modules must not maintain a second authentication state or implement a
token-refresh flow.

## Server state

Use the application query provider. Do not construct a feature-level
`QueryClient` or build a competing cache.

- Namespace query keys by feature, for example
  `["profile", userId]` or `["leaderboard", filters]`.
- Include every input that changes the response in the query key.
- Forward the query function's `AbortSignal` to the feature API operation.
- Retry only when the shared error reports `isRetryable`; never retry
  cancellation, authentication, authorization, validation, or protocol errors.
- Invalidate the smallest affected key after a successful mutation.
- Keep transient UI state in React state. Server records belong in the shared
  query cache rather than being copied into another global store.

## Authentication and routing

Wrap authenticated pages with `ProtectedRoute` and signed-out-only pages with
`PublicRoute`. Do not redirect during render or duplicate session checks in
feature components. The guards preserve safe local return locations and reject
external redirects.

Div owns the router registry. A feature owner supplies the page component,
required access level, and proposed path to Div instead of editing the registry
directly.

## Providers and configuration

Application-wide providers are composed once in `AppProviders`. Feature modules
consume provider hooks; they do not mount application providers themselves.
New environment values must be validated through `src/app/config.ts`. Do not
read `import.meta.env` throughout feature code.

## Validation

Feature changes include focused tests for successful, loading, empty, error,
and cancellation behavior where applicable. Before review, run the frontend
lint, type-check, unit-test, and production-build scripts provided by Div's
manifest. PR descriptions identify the issue and note contract, migration, and
rollback implications.
