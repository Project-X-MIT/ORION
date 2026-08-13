# Provider outage

Classify provider failures as bounded dependency timeouts. Keep accepted
PostgreSQL state, schedule an idempotent retry with exponential backoff, and
dead-letter after the configured budget. Redis loss must degrade to the
authoritative database path. Validate recovery with synthetic events before
resuming normal dispatch.
