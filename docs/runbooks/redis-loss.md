# Redis loss

Redis contains sessions, rate limits and disposable read caches only. On loss,
stop writes that require a session, rebuild the service from the pinned image,
expire stale cache keys, and verify PostgreSQL-backed reads. Reissue sessions
as needed; no accepted rating, settlement, notification or outbox mutation is
recovered from Redis.
