# Local development

The supported local environment is synthetic and disposable. It requires Docker
Compose and starts PostgreSQL, Redis, the API, worker, and frontend together:

```bash
cp .env.example .env
./scripts/dev.sh
```

The frontend is available at `http://localhost:5173`; its Caddy sidecar proxies
API and health requests over the internal Compose network. PostgreSQL uses the
`postgres_data` volume and Redis deliberately has persistence disabled. Redis
may be deleted or restarted at any time; PostgreSQL remains authoritative.

Apply migrations as a controlled step against an explicitly selected database:

```bash
DATABASE_URL=postgres://orion:orion@localhost:5432/orion ./scripts/migrate.sh
```

Do not put production credentials in `.env`, Compose files, images, or logs.
