import { Alert } from "../../shared/ui/Alert";
import { Button } from "../../shared/ui/Button";
import { LiveRegion } from "../../shared/accessibility/LiveRegion";
import { Pagination } from "../../shared/ui/Pagination";
import { VisuallyHidden } from "../../shared/accessibility/VisuallyHidden";
import { useAuth } from "../../providers/AuthProvider";

import { LeaderboardTable } from "./LeaderboardTable";
import { useLeaderboard } from "./hooks";

function formatAsOf(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Time unavailable";

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  return "The leaderboard is temporarily unavailable.";
}

export function LeaderboardPage() {
  const { user } = useAuth();
  const leaderboard = useLeaderboard();
  const data = leaderboard.data;
  const isInitialLoading = leaderboard.isPending && !data;
  const isInitialError = leaderboard.isError && !data;
  const isChangingPage = leaderboard.isChangingPage;
  const entries = data && !isChangingPage ? [...data.entries] : [];
  const currentUserEntry = entries.find((entry) => entry.user_id === user?.id);
  const isStale = Boolean(data && !isChangingPage && (leaderboard.isStale || leaderboard.isRefetchError));

  const pageStatus = isInitialLoading
    ? "Loading leaderboard"
    : isChangingPage
    ? `Loading leaderboard page ${leaderboard.page}`
    : `Leaderboard page ${leaderboard.page} loaded`;

  return (
    <main
      aria-busy={leaderboard.isFetching || undefined}
      style={{
        display: "grid",
        gap: "1rem",
        margin: "0 auto",
        maxWidth: "var(--ui-container-lg)",
        padding: "var(--ui-content-padding) var(--ui-layout-gutter)",
        width: "100%",
      }}
    >
      <header style={{ alignItems: "flex-start", display: "flex", flexWrap: "wrap", gap: "1rem", justifyContent: "space-between" }}>
        <div>
          <h1 style={{ margin: 0 }}>Global leaderboard</h1>
          <p style={{ color: "var(--ui-muted)", margin: ".35rem 0 0" }}>
            Current Elo rankings across ORION.
            {data ? ` Updated ${formatAsOf(data.as_of)}.` : ""}
          </p>
        </div>
        <Button
          aria-label="Refresh leaderboard"
          disabled={leaderboard.isFetching}
          isLoading={leaderboard.isFetching}
          loadingLabel="Refreshing leaderboard"
          onClick={() => void leaderboard.refetch()}
          variant="secondary"
        >
          Refresh leaderboard
        </Button>
      </header>

      <LiveRegion>
        <VisuallyHidden>{pageStatus}</VisuallyHidden>
      </LiveRegion>

      {isInitialError ? (
        <Alert title="Leaderboard unavailable" variant="danger">
          <p style={{ margin: 0 }}>{errorMessage(leaderboard.error)}</p>
          <div>
            <Button onClick={() => void leaderboard.refetch()} variant="secondary">Try again</Button>
          </div>
        </Alert>
      ) : null}

      {isStale ? (
        <Alert
          title="Leaderboard may be stale"
          variant={leaderboard.isRefetchError ? "danger" : "warning"}
        >
          <p style={{ margin: 0 }}>
            {leaderboard.isRefetchError
              ? "We could not refresh the latest rankings. Showing the last available page."
              : "This page may be out of date."}
          </p>
          {leaderboard.isRefetchError ? (
            <div>
              <Button disabled={leaderboard.isFetching} onClick={() => void leaderboard.refetch()} variant="secondary">
                Try again
              </Button>
            </div>
          ) : null}
        </Alert>
      ) : null}

      {data && !isChangingPage ? (
        <section
          aria-label="Your leaderboard position"
          style={{
            background: "var(--ui-surface)",
            border: "1px solid var(--ui-border)",
            borderRadius: "var(--ui-radius)",
            padding: "1rem",
          }}
        >
          <strong>Your position</strong>
          {currentUserEntry ? (
            <p style={{ margin: ".25rem 0 0" }}>
              You are ranked <strong>#{currentUserEntry.rank}</strong> with <strong>{currentUserEntry.rating} Elo</strong>.
            </p>
          ) : (
            <p style={{ margin: ".25rem 0 0" }}>Your position is not on this page.</p>
          )}
        </section>
      ) : null}

      {isInitialError ? (
        <p role="status">Try again to load the leaderboard results.</p>
      ) : (
        <LeaderboardTable
          currentUserId={user?.id ?? null}
          isLoading={isInitialLoading || isChangingPage}
          rows={entries}
        />
      )}

      <div style={{ alignItems: "center", display: "flex", flexWrap: "wrap", gap: "1rem", justifyContent: "space-between" }}>
        <p aria-live="polite" style={{ color: "var(--ui-muted)", margin: 0 }}>
          Page {leaderboard.page}
        </p>
        <Pagination
          ariaLabel="Leaderboard pages"
          busy={leaderboard.isFetching}
          hasNext={leaderboard.hasNextPage}
          hasPrevious={leaderboard.hasPreviousPage}
          onNext={leaderboard.goToNextPage}
          onPrevious={leaderboard.goToPreviousPage}
        />
      </div>
    </main>
  );
}
