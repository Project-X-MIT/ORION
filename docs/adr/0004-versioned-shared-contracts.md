# ADR 0004: Versioned shared contracts

- Status: Accepted
- Date: 2026-08-09
- Owners: divi912 and affected feature owners

## Context

ORION's independently owned features exchange user identifiers, ratings, API
responses, events, configuration and Redis coordination state. Unregistered
strings or feature-to-feature implementation dependencies make integration and
rolling deployment unsafe.

## Decision

Framework-neutral API/configuration primitives live in `orion-common`; domain
identifiers and versioned event payloads live in `orion-domain`; Redis key
patterns live in `orion-redis`. Each public identifier belongs to an in-code
registry with one owner. Documentation and deterministic golden fixtures are
checked by tests.

Additive changes may extend a compatible version. A breaking change receives a
new version or identifier and requires an ADR, affected-owner approval, a
migration window, dual-version fixtures and idempotent consumers.

## Consequences

Feature implementations depend on shared contracts instead of each other.
Contract changes have deliberate review overhead, but drift and silent breaking
changes fail locally and in CI. Redis remains disposable and events do not
replace authoritative PostgreSQL transactions.
