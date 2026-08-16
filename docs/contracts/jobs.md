# Worker job registry

The Rust worker registry is authoritative. Div registers stable job
identifiers and execution triggers; feature owners implement the module bodies
referenced by those registrations. Registration metadata must not contain
feature business logic.

The research review registration is integration-gated on DIV-06, DIV-08, and
PHANTOM-03. Those issue contracts remain owned by their respective owners and
are not duplicated here.

> Integration note: DIV-06 and DIV-08 are currently being implemented by Div.
> Connect their merged shared registrations to this job boundary after merge;
> do not treat this local registration metadata as completion of either issue.

| Job ID | Registration owner | Body owner | Body | Trigger |
| --- | --- | --- | --- | --- |
| `research_review` | divi912 | shivanshrawat13aug2007-commits | `orion_worker::jobs::research_review::process_research_award` | `orion.research.elo_award.requested` |
| `notification` | divi912 | divi912 | `orion_worker::jobs::notification::process_notification` | `orion.notification.requested` |

Worker runtime semantics—claims, retries, dead-letter handling, shutdown, and
observability—are shared execution concerns. The Phantom body owns research
eligibility and invokes the database-owned transaction; it does not register
the job or duplicate shared scheduler logic. Repeated delivery produces one
review/request effect; Yash's separate Elo consumer owns applying the award
and must enforce the award ledger idempotency key.

The worker runtime polls outbox rows with a queued or due-retry execution
state for registered triggers. It claims jobs through the durable execution
columns and invokes the registered body; the transport `status` column is
independent of this worker lifecycle.
The notification adapter claims only `orion.notification.requested` rows and
commits the inbox claim and notification upsert before acknowledging the
outbox row. This keeps Redis delivery hints best-effort while making the
PostgreSQL notification effect durable and idempotent.
Running jobs older than `WORKER_RUNNING_LEASE_SECONDS` are moved through the
same bounded retry path, so a process crash cannot leave a job running
forever. `Ctrl-C` and `SIGTERM` stop polling and close the PostgreSQL pool
within `SHUTDOWN_TIMEOUT_SECONDS`.
