# Deployment

Production promotes immutable API, worker, and frontend image digests through
staging and production. `infra/compose/docker-compose.prod.yml` does not build
images and requires image references, database/Redis URLs, CORS origins, and a
TLS domain from the deployment secret manager.

The release workflow is a protected, three-stage gate. The environment-approved
operator first runs the migration and provider deployment, then the workflow
probes the canary edge URL for the configured window. A failed readiness probe
immediately POSTs the previously approved image digests to the protected
`CANARY_ROLLBACK_HOOK_URL`; the hook must acknowledge within the recovery target
or the workflow fails closed. A successful canary is followed by immutable
edge/API smoke probes. The hook and all provider credentials remain environment
secrets; they are never stored in the repository.

Run migrations separately before application promotion:

```bash
DATABASE_URL="$DATABASE_URL" ./scripts/migrate.sh
```

Caddy terminates automatic HTTPS for `ORION_DOMAIN`; application and data
services are on the internal `backend` network and are not published directly.
The frontend is the only edge-published service. Health checks use
`/health/ready` and containers receive SIGTERM before the configured shutdown
deadline.

Rollback reuses the previous image digests and does not rebuild artifacts. A
forward-only migration must be compatible with the previous application during
the rollout window; if a migration cannot satisfy that window, stop promotion
and restore the isolated backup before retrying. Redis is rebuilt from
PostgreSQL after rollback and is never a source of accepted business state.

Before dispatching a release, validate the release-control registry:

```bash
tools/validate-feature-flags.sh
```

Every flag in `docs/release/feature-flags.json` is default-off, has a stable
owner, and has a future removal date. Enabling a flag still requires the owning
team's evidence and the protected environment approval.

The repeatable browser/accessibility/responsive matrix is defined in
`docs/release/uat-matrix.md` and runs in CI across Chromium, Firefox, WebKit,
and an iPhone-sized Chromium project. CI's matrix is synthetic unless a staging
fixture and owner approvals are explicitly attached to the release record.
