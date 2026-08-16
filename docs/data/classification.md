# Data classification

- **Restricted:** passwords, password hashes, session identifiers, provider
  credentials, tokens and raw research reports. Never log or export secrets.
- **Confidential:** email, profile fields, notifications, rating history and
  unpublished research. Export only to the authenticated account owner.
- **Internal:** audit events, operational identifiers and aggregate metrics.
- **Public:** published research projections, public leaderboard fields and
  health/metrics values without personal labels.

PostgreSQL is authoritative. Redis keys and WebSocket hints are disposable
copies and are rebuilt after loss.
