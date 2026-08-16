# DIV-16 release-gate evidence

Run date: 2026-08-16  
Environment: disposable synthetic HTTP/API fixtures on the developer machine;
no production credentials, data, registry promotion, or provider deployment.

## Controls validated

| Control | Result | Reproduction |
| --- | --- | --- |
| Feature flag registry | PASS: 4 flags, all `default=false`, unique owners, future removal dates | `FLAG_VALIDATION_DATE=2026-08-16 tools/validate-feature-flags.sh` |
| Canary failure stop/rollback | PASS: readiness HTTP 503 invoked the rollback hook at elapsed 0 seconds; hook acknowledged prior approved digests | `tools/canary-gate.sh` with a local failing canary and rollback fixture |
| Canary success window | PASS: two healthy probes completed a two-second synthetic window | `CANARY_WINDOW_SECONDS=2 CANARY_INTERVAL_SECONDS=1 tools/canary-gate.sh` |
| Post-deploy smoke | PASS: `/health/live`, `/health/ready`, and `/` returned HTTP 200 | `tools/post-deploy-smoke.sh` against the local healthy fixture |
| UAT/accessibility/responsive matrix | PASS: 12 release-gate tests across Chromium, Firefox, WebKit, and iPhone-sized Chromium; unrelated product E2E suites are run by their owning feature checks | `npm run test:e2e --workspace frontend -- --grep "release UAT matrix"` |

The failure-path canary command returned exit status 1 after the rollback hook
accepted the previous immutable image digests. This is intentional: promotion
stops after rollback and must not continue automatically.

## Linux VM verification rerun

On 2026-08-16 UTC (2026-08-17 IST), the same controls were rerun on the
disposable Ubuntu host `orion-vm` at commit `f6ee12d` on `Shaurya's-Branch`.
The raw staging log remains on the VM at
`~/ORION/.staging-evidence/staging-issue25-linux-20260816.txt`.

- The API, worker and frontend images rebuilt successfully and the API became
  ready with healthy PostgreSQL and Redis dependencies.
- The leaderboard load test completed 1,501 requests with 0 failures; p95 was
  3.27 ms. The peak API test completed 2,594 iterations / 5,188 checks with
  0 failures; p95 was 168.54 ms.
- Redis loss preserved a 200 leaderboard response; PostgreSQL loss returned a
  safe 503 for registration; worker termination and API graceful shutdown
  both recovered.
- The feature-flag validator passed all four default-off flags, and the
  post-deploy smoke probe returned 200 for `/health/live`, `/health/ready`,
  and `/`.
- An isolated encrypted backup/restore drill passed with a 0-second RPO age,
  1-second restore (within the 3,600-second RTO), 21 public-table row-count
  matches (including 14,636 users and user ratings), matching constraints
  (84 primary, 59 unique, 20 foreign-key, 108 check), zero unvalidated
  foreign keys, and zero lifecycle/invariant violations. The target database
  was dropped after verification. Backup SHA-256 was
  `214f9fe755c4ce658dcac5dc95e20b19ca90134bedbec24d76e4b6294616de65`.

## Evidence boundaries

The UAT fixture covers the public authentication journeys and authenticated
application shell. It is not product-owner acceptance for all eight product
areas. The real staging browser matrix, provider deployment, production
canary, post-deploy smoke sign-off, and product/security/operations approvals
remain external release evidence and are not inferred from these synthetic
runs. Use `docs/release/signoff-template.md` for those records.
