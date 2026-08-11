# Global leaderboard

`GET /api/v1/leaderboard` returns a deterministic, cursor-paginated view of
current global Elo. The endpoint is public.

## Authority and ordering

`user_ratings.rating` is the sole source of current Elo. PostgreSQL calculates
rank with this complete ordering:

```sql
ORDER BY rating DESC, user_id ASC
```

Higher Elo sorts first. Equal Elo is resolved by immutable user UUID ascending.
Rank is the one-based `ROW_NUMBER()` over that order, making every position
unique and contiguous. Usernames, profile fields, rank history, and Redis state
never affect ordering or rank.

`leaderboard_rank_history` supplies optional movement from the latest completed
snapshot. It does not supply current rank or Elo.

The rank service also exposes newest-first completed history pages internally.
History uses the same opaque offset cursor rules and validates that persisted
current and previous ranks are positive, one-based values.

## Cursor contract

The API cursor is an opaque, versioned encoding of the next result offset. The
database applies that offset only after calculating the complete deterministic
order:

```sql
ORDER BY rating DESC, user_id ASC
LIMIT :limit
OFFSET :cursor_offset
```

Missing, malformed, overflowing, or unsupported cursor data is rejected. A
cursor is never interpreted as a rank, Elo, or snapshot identifier. Clients must
not construct cursors or rely on their encoded representation.

## Validation and errors

The domain contract shares validation across the HTTP and cache adapters:

- `limit` must be between 1 and 100.
- Decoded offsets must fit PostgreSQL's signed 64-bit offset.
- Current and historical ranks must be positive and one-based.
- Current Elo read from `user_ratings` must be non-negative.

Malformed cursors and invalid limits are client input errors. Invalid rank or
Elo rows are authoritative-data invariant failures and must not be cached.
Database availability failures remain service errors rather than being
misreported as validation errors.

## Response DTO

The success envelope's `data` contains:

```json
{
  "entries": [
    {
      "rank": 1,
      "user_id": "00000000-0000-0000-0000-000000000001",
      "username": "orion",
      "display_name": "Orion",
      "avatar_url": null,
      "rating": 2400,
      "rank_movement": 2
    }
  ],
  "next_cursor": "v1.eyJyYXRpbmciOjI0MDAsInVzZXJfaWQiOiIuLi4ifQ",
  "as_of": "2026-08-11T08:30:00Z"
}
```

`rank_movement` is positive for movement up, negative for movement down, zero
for no change, and `null` when no completed comparison snapshot exists.
`next_cursor` is `null` on the final page. Its string representation is opaque
and may change between API versions.

`as_of` is the time PostgreSQL produced the view. Cached responses preserve the
original value. Redis may serve the DTO only within the registered bounded TTL;
on a miss or stale entry the API queries PostgreSQL, refreshes the disposable
cache, and returns the database result. Cache contents never become rank or Elo
authority.

Leaderboard pages use the registered
`orion:v1:cache:leaderboard:{limit}:{offset}` key and a 60-second expiry. A read
checks both insertion age and the preserved PostgreSQL `as_of` age; either age
reaching 60 seconds makes the value a miss and schedules it for deletion. An
already-stale page is rejected on write, preventing reinsertion from extending
its authority window. Corrupt values are deleted and treated as misses. Redis
command failures are surfaced to orchestration, which may degrade to the
authoritative PostgreSQL path.

After a validated `orion.rating.updated` event, every leaderboard page observed
by the process is invalidated. After the snapshot transaction commits, the same
invalidation occurs only when rows were inserted; idempotent or backdated
no-op snapshots do not churn the cache. Event handlers may then store pages they
have re-queried through the PostgreSQL rank service. Refresh data is accepted
only through the normal freshness and validation checks. Pages not observed by
the current process cannot outlive the registered 60-second TTL and `as_of`
budget.
