# Advanced settlement outbox contract

This is the feature-owned internal outbox contract for Advanced settlement.
It is persisted with `outbox_events.schema_version = 1`, and every payload
also carries `schema_version: 1`. Each payload has a stable `dedupe_key`; the
worker protects insertion with a PostgreSQL advisory transaction lock and an
existing-row check. Repeated deliveries therefore create one settlement,
cache hint, notification request, and dead-letter record per attempt.

| Event type | Purpose | Dedupe key |
| --- | --- | --- |
| `orion.quiz.advanced.settled` | Announces committed attempt/rating results. | `advanced-settlement:{attempt_id}` |
| `orion.quiz.cache.invalidate` | Invalidates question projections after commit. | `advanced-settlement:{attempt_id}:cache` |
| `orion.notification.requested` | Requests the user's durable rating notification. | `advanced-settlement:{attempt_id}:notification` |
| `orion.quiz.advanced.settlement.dead_lettered` | Records terminal worker handling. | `advanced-settlement:{attempt_id}:dead-letter` |

The notification payload includes both `dedupe_key` for outbox insertion and
`deduplication_key` for the notification consumer. It contains no provider
secret or actual-value payload; provider facts remain in PostgreSQL rating
audit columns.

The shared domain registry, versioned payload types, fixtures, and typed
consumer routing are kept in sync with this table. The PostgreSQL outbox row
is the durable handoff and Redis/Pub/Sub remains only an optional hint.
