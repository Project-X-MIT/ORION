# Public profiles

`GET /api/v1/profiles/{user_id}` returns the public profile projection for an
active user. The route is public; authentication is not required because the
response is privacy-filtered at the domain DTO and SQL query boundaries.

## Authority and privacy

PostgreSQL is authoritative for `users`, `user_ratings`, immutable
`rating_events`, completed `quiz_attempts`, `leaderboard_rank_history`, and
published `research_papers`. Redis stores only the complete, bounded version-1
public DTO under `orion:v1:cache:profile:{user_id}` for 120 seconds. The cache
key is independent of `limit`; the API trims each history collection after
loading so one caller cannot make another caller's response too short. A
cache miss, invalid schema, malformed value, or Redis outage falls back to
PostgreSQL.

The response never selects or serializes email, password hashes, account
status, reviewer identity, private research content, or unpublished research.
Disabled and deleted accounts behave as `404 NOT_FOUND`, including when a
stale cache entry exists. A committed `orion.rating.updated` event invalidates
only the affected user's profile key after the event contract is validated.

## Response

```json
{
  "schema_version": 1,
  "user_id": "00000000-0000-0000-0000-000000000001",
  "username": "orion",
  "display_name": "Orion",
  "bio": "Public profile",
  "avatar_url": null,
  "rating": 1510,
  "global_rank": 4,
  "rank_movement": 2,
  "quizzes_completed": 12,
  "correct_answers": 40,
  "rating_history": [],
  "rank_history": [],
  "performance_history": [],
  "published_research": []
}
```

History collections are oldest-first and bounded by `limit` (default and
maximum 100). `rating_history` contains immutable before/after Elo changes;
`rank_history` contains completed snapshot positions; and
`performance_history` contains completed attempts only, so pending Advanced
settlements never appear as completed performance. `published_research`
contains the title, abstract, publication timestamp, evaluation score,
evaluated content version, and completed Elo award only.

The frontend renders all three histories as SVG sparklines and always includes
an equivalent screen-reader table and text summary. It performs no Elo or rank
calculation; all values come from this response.
