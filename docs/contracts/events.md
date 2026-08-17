# Domain event registry

Events use `EventEnvelope<T>`. The envelope supplies a unique event identifier,
stable event type, positive schema version, UTC occurrence time, producer and
typed payload. Consumers validate the envelope and persist the event identifier
in the PostgreSQL inbox before applying an effect, in the same transaction, so
retries are idempotent. A consumer must use its own stable consumer key; the
inbox also rejects reusing an event identifier with different contract
metadata.

| Event type | Owner | Current version | Purpose |
| --- | --- | --- | --- |
| `orion.rating.updated` | akaidk | 1 | Announce an already committed authoritative rating change. |
| `orion.notification.requested` | divi912 | 1 | Request durable notification creation with a deduplication key. |
| `orion.leaderboard.snapshot.completed` | ShauryaBijalwan | 1 | Durable completion/effect record for one deterministic hourly leaderboard snapshot. |
| `orion.quiz.advanced.submitted` | akaidk | 1 | Request provider-backed settlement of an accepted numeric Advanced attempt. |
| `orion.quiz.advanced.settled` | akaidk | 1 | Announce a committed Advanced score and rating result. |
| `orion.quiz.cache.invalidate` | akaidk | 1 | Request best-effort invalidation of committed Advanced question projections. |
| `orion.quiz.advanced.settlement.dead_lettered` | akaidk | 1 | Record terminal Advanced settlement handling without changing the pending attempt. |

Leaderboard snapshot events use the snapshot UUID as their outbox and Pub/Sub
event identity. The PostgreSQL outbox row remains pending until cache
invalidation and publication succeed; a retry replays only pending effects.
Consumers must deduplicate by `snapshot_id` and treat the event as a hint: the
authoritative ranks remain in `leaderboard_rank_history`.

Version 1 fixtures are stored in `docs/contracts/fixtures/`. Adding an optional
field with a defined default can remain within a version. Renaming/removing a
field, changing its type or meaning, or making an optional field required must
create a new event version and retain the old consumer during the migration
window.
