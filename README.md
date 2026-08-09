# ORION

ORION is a financial-learning platform built around basic and advanced
quizzes, auditable Elo ratings, research publishing, market news, beginner
learning, public profiles, and community access.

## Workspace

The Rust workspace has one integration package and six implementation crates:

| Package | Owner | Responsibility |
| --- | --- | --- |
| `orion-integration` | Div | Cross-crate composition and integration tests only |
| `orion-api` | Div | Axum application assembly, middleware, and HTTP routes |
| `orion-common` | Div | Framework-neutral configuration and transport primitives |
| `orion-db` | Feature table owner; merged by Div | SQLx models, queries, transactions, and append-only migrations |
| `orion-domain` | Contract files: Div; feature modules: feature owner | Pure contracts and business rules |
| `orion-redis` | Div; feature cache modules: feature owner | Disposable cache and coordination state |
| `orion-worker` | Scheduler: Div; job bodies: feature owner | Scheduled and asynchronous execution |

The React application lives in `frontend/`. Deployment and operational assets
live in `infra/`, `scripts/`, and `docs/`.

## Build and test

Install a Rust toolchain compatible with the workspace `rust-version`, a C
toolchain, Node.js, PostgreSQL, Redis, and Docker Compose. Then run:

```bash
cargo metadata --no-deps --format-version 1
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend and service commands are exposed through the scripts in `scripts/` as
their owning issues are completed. Environment variable names and safe local
defaults belong in `.env.example`; secret values must never be committed.

## Ownership and integration

Work is committed to the developer's existing named branch:

- `Divyam's-Branch`: platform, shared contracts, DB-01, infrastructure, and final integration
- `Yash's-Branch`: quiz, Elo, and settlement
- `Shaurya's-Branch`: frontend system, leaderboard, profiles, and rankings
- `Shivansh's-branch`: research workflow and publishing
- `Sudhanshu's-Branch`: news, learning, and community

`main` is the integration branch. Before starting an issue, fast-forward or
merge the current `main` into the named branch. Pull requests target `main` and
must pass required checks plus CODEOWNERS review. Div is the single writer for
root manifests, lockfiles, shared registries, workflow composition, migration
ordering, and release assets.

The expected merge order is:

1. Repository governance and versioned shared contracts.
2. DB-01, API/Redis foundations, and authentication.
3. Feature backends and idempotent worker jobs.
4. Shared frontend and feature UI integration.
5. Outbox, observability, security, performance, recovery, and release gates.

Migrations are forward-only after reaching `main`. Redis is never authoritative
for ratings, attempts, reviews, progress, content, or notifications. No feature
may mutate current rating outside the shared atomic rating transaction.

See [AGENTS.md](AGENTS.md) for the contributor rules enforced during changes.
