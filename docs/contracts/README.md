# ORION contract policy

The Rust registries are authoritative; these documents explain their meaning
and are checked by tests so an identifier cannot be added without an owner and
documentation.

## Lifecycle

Every API, event, configuration, and Redis contract follows this lifecycle:

1. Draft the identifier, owner, version, validation, and fixture.
2. Obtain review from Div and every affected feature owner.
3. Approve and publish the version before implementation consumes it.
4. Extend compatibly by adding optional behavior or a new identifier.
5. Deprecate with telemetry, a migration path, and a stated removal release.
6. Remove only after the migration window and an approved ADR.

Changing the meaning, serialized name, required fields, value type, route
semantics, or Redis ownership of an existing contract is breaking. A breaking
change requires a new version or identifier, an ADR, golden fixtures for both
versions during the compatibility window, and approval from every affected
owner.

## Dependency direction

Feature modules consume `orion-domain` and `orion-common` contracts. They do not
depend on another feature's route, repository, cache, or worker implementation.
PostgreSQL remains authoritative; events and Redis coordinate work but do not
replace committed business state.

## Enforcement

Contract tests enforce deterministic golden serialization, non-empty ownership,
unique identifiers and registry-to-documentation coverage. CI runs these tests
through `cargo test --workspace`.
