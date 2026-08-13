# Product traceability and approval record

| Area | Contract/test evidence | Owner approval |
|---|---|---|
| Leaderboard | `crates/orion-api/src/routes/leaderboard.rs`, performance SQL | product / feature |
| Basic quiz | quiz contract, settlement tests | product / feature |
| Advanced quiz | advanced settlement tests and Elo ledger | product / feature |
| Elo | rating ledger invariants and reconciliation runbook | data / feature |
| Research | lifecycle, review, outbox and worker recovery tests | research / feature |
| News | ingestion models and worker tests | feature |
| Learning | progress repository and API tests | feature |
| Discord | integration contract/runbook when provider is enabled | feature |

Release candidates must attach CI run IDs, synthetic load/restore reports,
security scan/SBOM/provenance attestations, migration and rollback rehearsal
IDs, and explicit product/security/operations approvals. Unresolved contract or
critical/high security findings block promotion.
