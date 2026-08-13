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

Version 1 fixtures are stored in `docs/contracts/fixtures/`. Adding an optional
field with a defined default can remain within a version. Renaming/removing a
field, changing its type or meaning, or making an optional field required must
create a new event version and retain the old consumer during the migration
window.
