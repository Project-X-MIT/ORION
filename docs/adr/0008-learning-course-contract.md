# ADR 0008: Beginner learning course and resumable progress contract

- Status: Proposed; integration-gated
- Date: 2026-08-16
- Owners: Sudhanshu learning feature owner; DB owner for persistence; Div for route/cache registration
- Dependencies: DIV-04 through DIV-06 and DB-01 integration

This ADR records the contract implemented against the completed DB-05 learning
schema. It does not infer Product publication approval, a future `courses`
table, or Div's route and cache registration.

## Context

The beginner course is currently represented by the published rows in
`course_modules` and `course_lessons`. The DB-05 baseline has no `courses`
table, so the API uses the reserved logical identity
`BEGINNER_COURSE_ID = 00000000-0000-0000-0000-000000000001` and rejects other
course IDs until DB-01 supplies an approved course identity.

Learner progress is stored in `course_progress` and must remain resumable
across logout, Redis loss, and another device. Published content may use a
disposable Redis projection, but Redis must never become the authority for
progress or publication.

## Decision

### Content and progress

- Modules are returned by `(display_order, id)`.
- Lessons are returned by `(lesson_order, id)` within their published module.
- Draft or retired modules and lessons are not learner-visible.
- Prerequisites are the earlier published lessons in that deterministic order.
- Completion is monotonic and idempotent. Replaying a completion preserves the
  first `completed_at` and cannot create a second `(user_id, lesson_id)` row.
- The next lesson is the first incomplete published lesson whose prerequisites
  are complete. An empty published course is not reported as completed.

### Persistence, compatibility, and rollback

This issue adds no migration and edits no applied migration. The persistence
baseline remains `202608070008_learning.sql`, owned by the DB owner. Its
existing indexes are the compatibility baseline:

- `course_modules_published_order_idx` for published module ordering;
- `course_lessons_module_published_order_idx` for published lesson ordering;
- `course_progress_user_updated_at_idx` for user progress reads.

The domain contract and learning cache envelope are versioned at `1`. Additive
optional response fields are compatible. Changing required fields, ordering,
publication semantics, the logical course identity, or the cache envelope
requires a new contract version and an explicit compatibility window. Div must
own any migration ordering or shared registry update.

Because no schema changed, rollback is an application rollback only: deploy the
previous application version and leave the DB-05 tables in place. Deleting the
learning Redis key is safe and recoverable because Redis is disposable. A future
course table or publication migration must be forward-only, include an upgrade
and rollback plan, and must not edit this applied baseline migration.

### Authority and cross-feature effects

PostgreSQL is authoritative for published content and all learner progress.
Redis stores only validated, published, mostly-static content with a bounded
TTL. Cache invalidation is requested only after an authorized publication
transaction commits; Redis does not authorize publication.

This feature performs no rating change and writes no Elo ledger entry. It emits
no cross-feature event and therefore has no learning-specific event consumer.
If a future learning effect crosses feature boundaries, it must use the shared
versioned event registry and an idempotent consumer; direct rating writes are
not permitted.

### Operations, logs, and fixtures

Operational logs use generic cache-refill diagnostics only. They do not include
credentials, tokens, cookies, progress payloads, or unnecessary user data.

The reproducible content fixture is
`crates/orion-db/seeds/learning_content.sql`. Contract and failure evidence is
covered by:

- `crates/orion-domain/src/learning.rs` domain tests;
- `crates/orion-api/src/routes/learning.rs` projection, authorization-boundary,
  empty, and Redis-fallback tests;
- `crates/orion-redis/src/cache/learning.rs` publication and cache-envelope
  tests;
- `crates/orion-db/tests/learning_progress.rs` fresh isolated-schema,
  unpublished-content, replay, Redis-loss, and concurrency tests.

The isolated DB tests call the completed migration set through
`orion_db::pool::migrate`. Since this issue does not change schema inputs, a
separate learning migration-upgrade test is not applicable; the existing DB
upgrade suite remains part of `cargo test -p orion-db --tests`.

## Evidence and unavailable approvals

The relevant Rust formatting, Clippy, domain, API, Redis, DB integration, and
contract checks pass locally. The learning feature remains unexported and
unmounted in shared Div-owned registries until DIV-04 through DIV-06 land; its
feature tests were compiled with temporary local exports and those exports were
restored. Product publication approval and the DB-01 course-table identity are
intentionally unavailable evidence, not inferred approvals.
