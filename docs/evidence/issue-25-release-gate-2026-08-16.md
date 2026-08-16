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

## Evidence boundaries

The UAT fixture covers the public authentication journeys and authenticated
application shell. It is not product-owner acceptance for all eight product
areas. The real staging browser matrix, provider deployment, production
canary, post-deploy smoke sign-off, and product/security/operations approvals
remain external release evidence and are not inferred from these synthetic
runs. Use `docs/release/signoff-template.md` for those records.
