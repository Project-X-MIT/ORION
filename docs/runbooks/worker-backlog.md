# Worker backlog

Page when pending outbox events exceed 100 for ten minutes. Inspect counts by
event type and job state; never print payloads. Scale workers, verify leases
are advancing, then replay only dead letters with an approved event ID. A
replay preserves the original event identity and inbox deduplication key.
