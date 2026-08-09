# Configuration registry

Only keys in `orion_common::config::CONFIG_KEYS` may be read by application
code. Values classified as secret must never appear in logs, error responses,
metrics, images, or committed fixtures. Production injects secrets at runtime.

| Key | Owner | Secret class | Validation and use |
| --- | --- | --- | --- |
| `APP_ENV` | divi912 | Public | Non-empty environment name. |
| `API_BIND_ADDRESS` | divi912 | Public | Socket address for the API listener. |
| `DATABASE_URL` | divi912 | Secret | PostgreSQL connection URL. |
| `DATABASE_MAX_CONNECTIONS` | divi912 | Public | Positive pool limit. |
| `REDIS_URL` | divi912 | Secret | Redis connection URL. |
| `SESSION_TTL_SECONDS` | divi912 | Public | Positive session lifetime. |
| `SESSION_COOKIE_SECURE` | divi912 | Public | Boolean secure-cookie policy. |
| `CORS_ALLOWED_ORIGINS` | divi912 | Sensitive | Comma-separated trusted origins. |
| `REQUEST_TIMEOUT_SECONDS` | divi912 | Public | Positive request deadline. |
| `RUST_LOG` | divi912 | Public | Tracing filter expression. |
| `LOG_FORMAT` | divi912 | Public | `pretty` locally or `json` operationally. |
| `DISCORD_INVITE_URL` | sudhanshu001122 | Sensitive | Valid invite URL. |
| `NEWS_API_BASE_URL` | sudhanshu001122 | Public | Valid provider base URL. |
| `NEWS_API_KEY` | sudhanshu001122 | Secret | Non-empty provider credential. |

A new key must be registered, documented, represented safely in
`.env.example`, validated at the owning process boundary, and classified before
use. Renaming or removing a production key requires a dual-read migration
window.
