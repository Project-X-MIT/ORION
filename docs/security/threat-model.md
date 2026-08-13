# ORION threat model

| Asset / boundary | Threat | Control and evidence |
|---|---|---|
| Authentication/session | credential stuffing, fixation, cookie theft | Argon2id, rate limits, rotated session IDs, HttpOnly/Secure production cookies, generic errors |
| Elo/rating ledger | replay or client-supplied score manipulation | server-side settlement, immutable ledger, inbox deduplication and reconciliation |
| Research uploads/provider | SSRF, unsafe files, provider outage | URL/content policy, bounded body size, no binary upload route, timeout/retry/dead-letter |
| Redis/cache/WebSocket | cache poisoning, cross-user delivery, outage | explicit CORS, recipient filtering, Redis never authoritative, bounded broadcast channel |
| Admin actions | IDOR or privilege escalation | authenticated owner checks, role gates, protected audit rows, review/CODEOWNERS |
| Database/supply chain | destructive migration, compromised artifact | forward-only migrations, lockfile/audit/security CI, immutable digest promotion |

Findings follow `detected -> triaged -> fixed or expiring risk acceptance ->
verified -> closed`. A critical/high finding blocks release unless Div records
an owner, expiry, compensating control and approver.
