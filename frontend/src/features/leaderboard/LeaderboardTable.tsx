import { DataTable } from "../../shared/tables/DataTable";

import type { LeaderboardEntry } from "./types";

type LeaderboardTableProps = Readonly<{
  currentUserId: string | null;
  isLoading?: boolean;
  rows: readonly LeaderboardEntry[];
}>;

function formatRating(rating: number): string {
  return String(rating);
}

function movementContent(movement: number | null) {
  if (movement === null) {
    return <span aria-label="No previous rank snapshot" data-testid="leaderboard-movement-none">—</span>;
  }

  if (movement === 0) {
    return <span aria-label="No rank change" data-testid="leaderboard-movement-unchanged">0</span>;
  }

  const direction = movement > 0 ? "up" : "down";
  const places = Math.abs(movement);
  return (
    <span
      aria-label={`${direction} ${places} ${places === 1 ? "place" : "places"}`}
      data-testid="leaderboard-movement"
    >
      {movement > 0 ? `↑ ${places}` : `↓ ${places}`}
    </span>
  );
}

function playerContent(row: LeaderboardEntry, currentUserId: string | null) {
  const isCurrentUser = row.user_id === currentUserId;
  const handle = row.username.trim() || "Unknown handle";
  const displayName = row.display_name?.trim();

  return (
    <span style={{ display: "grid", gap: ".15rem", minWidth: "10rem" }}>
      <span style={{ alignItems: "center", display: "flex", flexWrap: "wrap", gap: ".5rem" }}>
        <strong>{handle}</strong>
        {isCurrentUser ? (
          <span
            aria-label="Authenticated user"
            data-current-user="true"
            data-testid="leaderboard-current-user"
            style={{
              border: "1px solid currentColor",
              borderRadius: "999px",
              fontSize: ".75rem",
              fontWeight: 700,
              padding: ".1rem .45rem",
            }}
          >
            You
          </span>
        ) : null}
      </span>
      {displayName && displayName !== handle ? (
        <span style={{ color: "var(--ui-muted)", fontSize: ".875rem" }}>{displayName}</span>
      ) : null}
    </span>
  );
}

export function LeaderboardTable({ currentUserId, isLoading = false, rows }: LeaderboardTableProps) {
  return (
    <DataTable
      caption="Global leaderboard"
      columns={[
        {
          align: "right",
          header: "Rank",
          id: "rank",
          render: (row) => <span aria-label={`Rank ${row.rank}`}>{row.rank}</span>,
          width: "5rem",
        },
        {
          header: "Handle",
          id: "handle",
          render: (row) => playerContent(row, currentUserId),
        },
        {
          align: "right",
          header: "Elo",
          id: "rating",
          render: (row) => <span>{formatRating(row.rating)}</span>,
          width: "6rem",
        },
        {
          align: "right",
          header: "Movement",
          id: "movement",
          render: (row) => movementContent(row.rank_movement),
          width: "7rem",
        },
      ]}
      emptyMessage="No ranked players are available yet."
      getRowId={(row) => row.user_id}
      isLoading={isLoading}
      loadingRowCount={5}
      rows={[...rows]}
    />
  );
}
