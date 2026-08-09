# Domain event registry

Events use `EventEnvelope<T>`. The envelope supplies a unique event identifier,
stable event type, positive schema version, UTC occurrence time, producer and
typed payload. Consumers persist the event identifier before applying an effect
so retries are idempotent.

| Event type | Owner | Current version | Purpose |
| --- | --- | --- | --- |
| `orion.rating.updated` | akaidk | 1 | Announce an already committed authoritative rating change. |
| `orion.notification.requested` | divi912 | 1 | Request durable notification creation with a deduplication key. |

Version 1 fixtures are stored in `docs/contracts/fixtures/`. Adding an optional
field with a defined default can remain within a version. Renaming/removing a
field, changing its type or meaning, or making an optional field required must
create a new event version and retain the old consumer during the migration
window.
