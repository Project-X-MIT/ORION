# Deployment

Production promotes immutable API, worker, and frontend image digests through
staging and production. `infra/compose/docker-compose.prod.yml` does not build
images and requires image references, database/Redis URLs, CORS origins, and a
TLS domain from the deployment secret manager.

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
