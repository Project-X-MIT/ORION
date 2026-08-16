# Product traceability and approval record

| Area | Contract/test evidence | Release decision | Owner approval |
|---|---|---|---|
| Leaderboard | `crates/orion-api/src/routes/leaderboard.rs`, performance SQL | covered; attach staging run | product / feature |
| Basic quiz | quiz contract, settlement tests | covered; attach staging run | product / feature |
| Advanced quiz | advanced settlement tests and Elo ledger | covered; attach staging run | product / feature |
| Elo | rating ledger invariants and reconciliation runbook | covered; attach reconciliation run | data / feature |
| Research | lifecycle, review, outbox and worker recovery tests | covered; attach UAT fixture | research / feature |
| News | ingestion models and worker tests | covered; attach provider test | feature |
| Learning | progress repository and API tests | covered; attach UAT fixture | feature |
| Discord | integration contract/runbook when provider is enabled | disabled by default; record provider decision | feature |

Release candidates must attach CI run IDs, synthetic load/restore reports,
security scan/SBOM/provenance attestations, migration and rollback rehearsal
IDs, and explicit product/security/operations approvals. Unresolved contract or
critical/high security findings block promotion.

The repository evidence demonstrates contract coverage and synthetic staging
behavior, but does not manufacture owner decisions. Use
`docs/release/signoff-template.md` to record the external decisions required to
turn this traceability map into a production release approval.
