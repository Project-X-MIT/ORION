## Issue and scope

- Closes/advances:
- Owner branch:
- Files or contracts intentionally not changed:

## What changed

Describe the behavior, contract, schema, or operational change.

## Why

Explain the problem and the chosen boundary or tradeoff.

## Validation

- [ ] Rust formatting passes.
- [ ] Rust Clippy passes with warnings denied.
- [ ] Relevant Rust unit/integration and migration tests pass.
- [ ] Frontend lint, type-check, tests, and build pass when applicable.
- [ ] Contract/security/container checks pass when applicable.
- [ ] Authorization, validation, empty, failure, retry, and concurrency paths are covered.

List the exact commands and important results:

```text

```

## Data, compatibility, and operations

- [ ] No existing migration was edited.
- [ ] New migration ownership, indexes, compatibility window, and rollback implications are documented.
- [ ] Redis remains disposable and PostgreSQL remains authoritative.
- [ ] Rating changes use the shared atomic transaction and immutable ledger.
- [ ] Events are versioned and consumers are idempotent.
- [ ] Logs and telemetry contain no secrets or unnecessary personal data.
- [ ] Documentation and fixtures match the implemented contract.

## Reviewer evidence

- Required CODEOWNERS:
- Screenshots/reports/artifacts:
- Known follow-up issues:
