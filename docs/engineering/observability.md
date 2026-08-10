# Observability and data minimization

ORION uses structured JSON logs for operational events. HTTP tracing records
only the request method, validated request ID, response status, and timing;
the URI query string, headers, cookies, authorization values, and request or
response bodies are excluded.

Authentication events correlate with the request ID rather than logging email,
username, session IDs, or other user identifiers. Passwords, password hashes,
database and Redis URLs, API keys, report contents, notification bodies, and
raw request payloads must never be emitted to logs, metrics, or traces.

Errors exposed to clients and operational logs use stable generic messages;
the underlying database or session error is not serialized into an API
response. Add a field only when it is required to diagnose the operation and
is not personal or secret data.
