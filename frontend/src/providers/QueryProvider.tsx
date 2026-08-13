import {
  MutationCache,
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import type { PropsWithChildren } from "react";

import { isApiClientError } from "../shared/api/errors";

const STALE_TIME_MS = 30_000;
const GARBAGE_COLLECTION_TIME_MS = 5 * 60_000;
const MAX_QUERY_RETRIES = 2;

function publishQueryError(error: unknown) {
  if (isApiClientError(error) && error.kind === "cancelled") return;
  window.dispatchEvent(new CustomEvent("orion:query-error", { detail: error }));
}

export function shouldRetryQuery(failureCount: number, error: unknown): boolean {
  return isApiClientError(error) && error.isRetryable && failureCount < MAX_QUERY_RETRIES;
}

export const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: publishQueryError }),
  mutationCache: new MutationCache({ onError: publishQueryError }),
  defaultOptions: {
    queries: {
      staleTime: STALE_TIME_MS,
      gcTime: GARBAGE_COLLECTION_TIME_MS,
      retry: shouldRetryQuery,
      retryDelay: (attempt) => Math.min(1_000 * 2 ** attempt, 30_000),
      refetchOnWindowFocus: false,
      refetchOnReconnect: true,
    },
    mutations: {
      retry: false,
    },
  },
});

const isDevelopment = (import.meta as ImportMeta & {
  readonly env: { readonly DEV?: boolean };
}).env.DEV === true;

export function QueryProvider({ children }: PropsWithChildren) {
  return (
    <QueryClientProvider client={queryClient}>
      {children}
      {isDevelopment ? <ReactQueryDevtools initialIsOpen={false} /> : null}
    </QueryClientProvider>
  );
}
