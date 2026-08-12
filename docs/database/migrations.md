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
compatible. Rollback is forward-only: deploy a compensating application
release rather than removing the guard from a database that has recorded this
migration.
