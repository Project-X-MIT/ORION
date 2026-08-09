# Redis key registry

All keys and channels start with `orion:v1`. Components construct keys through
typed helpers; feature code must not invent raw namespaces. Redis is disposable:
cache misses fall back to PostgreSQL, locks protect retryable transactions, and
Pub/Sub messages are only delivery hints for durable outbox-backed state.

| Registry ID | Owner | Pattern | TTL/invalidation |
| --- | --- | --- | --- |
| `session` | divi912 | `orion:v1:session:{session_id}` | Configured session TTL; delete on logout/revocation. |
| `rate_limit.login` | divi912 | `orion:v1:rate_limit:login:{subject_hash}` | 15 minutes; automatic expiry. |
| `lock.advanced_settlement` | akaidk | `orion:v1:lock:advanced_settlement:{attempt_id}` | Bounded lease; release after settlement. |
| `lock.worker_job` | divi912 | `orion:v1:lock:worker_job:{job_name}` | Bounded lease; release after run. |
| `cache.quiz_question` | akaidk | `orion:v1:cache:quiz_question:{question_id}` | 5 minutes; invalidate after mutation. |
| `cache.leaderboard` | ShauryaBijalwan | `orion:v1:cache:leaderboard:{limit}:{offset}` | 1 minute; invalidate after rating/snapshot commits. |
| `cache.profile` | ShauryaBijalwan | `orion:v1:cache:profile:{user_id}` | 2 minutes; invalidate after profile/rating/rank commits. |
| `cache.research` | shivanshrawat13aug2007-commits | `orion:v1:cache:research:{research_id}` | 5 minutes; only published research is cached. |
| `cache.news_feed` | sudhanshu001122 | `orion:v1:cache:news_feed:{limit}:{offset}` | 2 minutes; invalidate after ingestion commits. |
| `cache.learning_course` | sudhanshu001122 | `orion:v1:cache:learning_course:{course_id}` | 1 hour; invalidate after content commits. |
| `pubsub.notification` | divi912 | `orion:v1:pubsub:notification` | Ephemeral channel; PostgreSQL/outbox is durable. |
| `pubsub.rating` | akaidk | `orion:v1:pubsub:rating` | Ephemeral channel; PostgreSQL rating is authoritative. |

Changing a key pattern creates a new versioned namespace. During migration,
readers may fall back to the previous namespace, while writers populate both;
the previous key is removed only after its maximum TTL and deployment window.
