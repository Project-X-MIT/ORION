# Database migrations

ORION migrations are append-only after they reach `main`. In particular,
`202608070001`, `202608070002`, and `202608070009` are immutable empty legacy
versions that may already be recorded in deployed databases.

Application startup must call `orion_db::pool::connect_migrate_and_validate` (or
call `migrate` on an existing pool). The migrator runs the idempotent
`202608090010_users_foundation.sql` compatibility preflight under a PostgreSQL
advisory lock before SQLx processes the recorded chain. This supplies the
`users` prerequisite needed by already-merged DB-02 through DB-05 migrations,
then SQLx records version `202608090010` normally after the legacy versions.
Running the raw migration directory on a completely empty database without this
preflight is unsupported because the historical DB-01 slot cannot be edited.

Database tests use a dedicated `ORION_TEST_DATABASE_URL`, create a unique schema
per test, and drop only that schema. The acceptance suite covers a fresh chain,
an upgrade with the empty legacy versions already recorded, uniqueness error
mapping, idempotent notification reads, repeat-safe seeds, and feature migration
regressions.

Migration `202608100001_rating_ledger.sql` is owned by the database/rating
transaction owner. It adds the append-only user rating ledger and its index;
the trigger rejects updates and deletes. Existing `user_ratings` and
`rating_events` data remain compatible. Rollback is forward-only: deploy a
compensating application release rather than deleting ledger rows.

Migration `202608100002_event_consumptions.sql` adds the event-consumer inbox.
Its `(consumer_key, event_id)` primary key makes redelivery idempotent, while
the stored event type and schema version reject event-id collisions. Existing
feature tables are unchanged; rollback is forward-only and leaves consumed
event history intact.

Migration `202608130004_research_published_immutability.sql` is owned by the
research feature and adds a database trigger preventing edits to published
paper content. Drafts remain editable, and existing research rows are
compatible. It adds no indexes because the guard only evaluates the row being
updated. The compatibility window is the normal rolling deployment window:
old readers remain compatible, while writers must be upgraded before the
trigger is enabled. Rollback is forward-only: deploy a compensating
application release rather than removing the guard from a database that has
recorded this migration.

Migration `202608120001_outbox_job_metadata.sql` adds the worker-owned
execution lifecycle beside the existing outbox transport status. The job
status, attempts, retry schedule, and dead-letter context are indexed for
operational polling; existing outbox rows remain transport-compatible.

Migration `202608160001_advanced_actual_settlement.sql` is owned by the
Advanced Quiz feature owner, with Div responsible for merge ordering. It is
forward-only and adds nullable provider-fact, decimal-error, source-metadata,
and Elo-policy columns to `rating_events`; it also permits the approved
neutral-zone `K = 0` and adds a partial source lookup index. Existing Basic
writers and readers remain compatible because they continue to use the
original non-null rating columns and ignore the nullable Advanced projection.
The new worker must not be promoted before this migration is recorded; old
readers may remain during the rolling compatibility window. Rollback is by a
compensating application release and Redis rebuild from PostgreSQL; the
applied migration and its audit columns are never removed.

Migration `202608120002_research_content_immutability.sql` adds the database
guard for research title, abstract, and content after submission. It protects
the evaluated/publication version from direct SQL writers, including changes
that would otherwise leave `published_at` unchanged and bypass cache version
checks. Rollback is forward-only; the trigger is removed only by a compensating
migration after an explicit policy decision.
