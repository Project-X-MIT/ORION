# ORION contributor instructions

These rules apply to the entire repository.

## Branch and review workflow

- Work on the developer's existing named branch; do not create competing
  integration branches without team approval.
- Bring `main` into the named branch before starting a new issue and before
  requesting final review.
- Keep commits scoped to one issue or one independently reviewable dependency.
- Pull requests target `main`, identify the issue, and include validation and
  migration/rollback implications.
- Do not bypass required CI checks or CODEOWNERS review.

## Single-writer paths

Div is the only direct writer for root Cargo manifests and lockfiles,
`src/lib.rs`, `AGENTS.md`, `README.md`, `.github/CODEOWNERS`, shared workflow
composition, shared contract registries, migration ordering, `infra/**`, and
release scripts. Feature owners propose changes to these paths through Div.

Feature owners retain responsibility for their domain modules, routes, DB
models/queries/transactions, worker job bodies, frontend feature modules, and
tests. Shared files must not be used to hide feature-specific business logic.

## Architecture boundaries

- `orion-domain` contains pure contracts and business rules; it does not depend
  on Axum, SQLx, or Redis implementations.
- `orion-common` contains framework-neutral primitives only. Business logic
  belongs to the owning domain module.
- `orion-api` composes isolated feature routers. Feature routers do not assemble
  the application or global middleware.
- `orion-worker` owns scheduling, retries, claims, and shutdown semantics;
  feature owners provide idempotent job bodies.
- PostgreSQL is authoritative. Redis loss must not lose accepted business state.
- Elo changes use the shared atomic transaction and immutable rating ledger.
- Cross-feature asynchronous effects use versioned events and idempotent
  consumers; they do not call unrelated implementation modules directly.

## Database rules

- Every table and migration has one owner. Div merges migration ordering.
- Never edit a migration that has reached `main`, including legacy empty
  migrations. Add a new, ordered, forward-only migration.
- Schema changes ship with their model, query/transaction, tests, indexes,
  compatibility window, and rollback implications.
- Test fresh migration and upgrade paths. Retries and concurrent execution must
  not duplicate a rating, settlement, review, progress update, or notification.

## Required validation

Run the relevant subset locally and ensure CI runs the complete set:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend changes also require lint, type-check, unit tests, and production
build. Contract, migration, security, and container checks are mandatory when
their inputs change. Do not log credentials, tokens, cookies, report contents,
or unnecessary personal data.
