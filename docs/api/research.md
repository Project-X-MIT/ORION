# Research API

## Integration dependencies

The research author, reviewer, and public experiences are integration-gated on
`SHAURYA-01` and `SHAURYA-02` for shared frontend accessibility, form, and UI
primitives, and on `PHANTOM-02` through `PHANTOM-04` for the authoritative
research lifecycle contracts. This feature consumes those contracts; it does
not reimplement shared components, router registration, endpoint registries,
or server-owned lifecycle transitions.

The machine-readable OpenAPI fragment for these routes is available at
[research.openapi.yaml](research.openapi.yaml).

All responses use the version 1 envelope:

```json
{
  "api_version": 1,
  "request_id": "<uuid>",
  "data": {}
}
```

Research papers are created as `draft`. Only the author who owns a draft may
edit or submit it. Ownership failures and attempts to access another user's
unpublished paper return the same `404 NOT_FOUND` response, so the API does
not disclose whether an identifier belongs to another author.

Author-only responses and authenticated detail reads include
`Cache-Control: private, no-store`; private drafts are not eligible for shared
HTTP caching.

Public research responses are also `Cache-Control: no-store`. The controlled
server-side Redis cache is the only public-read acceleration layer, because a
withdrawal cannot reliably purge browser or intermediary HTTP caches.

The research Redis cache is an acceleration layer for anonymous published
reads only. PostgreSQL remains authoritative for drafts, review records and
decisions, and Elo award state. Redis cache contents must never be used to
authorize a lifecycle transition, complete a review, or settle an award.
Redis read, invalidation, and cache-fill failures are non-fatal for public
research reads: the route logs the operational failure and falls back to
PostgreSQL.

The research review worker does not reimplement research rules or open a
business transaction. `orion-db` owns publication/review eligibility,
persisted-evaluation validation, award-request idempotency, and the durable
outbox insert. The worker invokes that operation and owns only delivery
claims, bounded retries, dead-letter metadata, tracing, and metrics.

## Author lifecycle

### Create a draft

`POST /api/v1/research`

Requires the `orion_session` authentication cookie.

Request body:

```json
{
  "title": "A research title",
  "abstract": "A short summary",
  "content": "The full research paper"
}
```

The response contains the new paper with `status: "draft"`.

Research authoring accepts `application/json` only. Requests larger than 1 MiB
are rejected. The plain-text policy limits `title` to 200 characters,
`abstract` to 5,000 characters, and `content` to 500,000 characters. Leading
and trailing whitespace is removed and CRLF line endings are normalized. Null
bytes, other disallowed control characters, HTML-like markup, and dangerous URL
schemes are rejected. Binary file uploads are not supported by these routes.

### List the author's drafts

`GET /api/v1/research/drafts?limit=20&offset=0`

Requires the owning author's `orion_session` authentication cookie. The
response uses the same paginated shape as the public catalogue, but contains
only the authenticated author's papers whose status is `draft`. Other authors'
drafts and papers that have already been submitted are not included.

### Retrieve the author's paper or status

`GET /api/v1/research/{research_id}`

An authenticated author may retrieve their own paper in any lifecycle state,
including the current `status` after submission. An authenticated user who is
not the author can only retrieve the paper after it is published.

### Update a draft

`PUT /api/v1/research/{research_id}`

Requires the owning author's session and accepts the same body as draft
creation. Updates are accepted only while the paper is in `draft` state;
submitted, reviewed, rejected, approved, and published papers are immutable
through this endpoint.

### Submit a draft

`POST /api/v1/research/{research_id}/submission`

Requires the owning author's session. A successful submission changes the
paper status to `submitted` and records `submitted_at`. The repository applies
the state transition conditionally, so stale or repeated submissions cannot
skip the lifecycle.

### Read review status and feedback

`GET /api/v1/research/{research_id}/reviews`

Requires the owning author's `orion_session` authentication cookie. The
response contains reviewer decisions for the author's paper without exposing
reviewer identities or award bookkeeping:

```json
{
  "reviews": [
    {
      "score": 72,
      "recommendation": "reject",
      "comments": "Please clarify the sampling method.",
      "evaluation": {
        "overall_score": 72,
        "recommendation": "reject",
        "rationale": "The result is promising but the methodology needs clarification.",
        "strengths": ["Clear research question"],
        "concerns": ["Sampling method is underspecified"],
        "evidence": []
      },
      "reviewed_at": "<timestamp>"
    }
  ]
}
```

Authors receive an empty `reviews` array while a submitted or under-review
paper has not received a decision. Other callers receive the same `404
NOT_FOUND` response as private paper reads.

## Public catalogue

### List published research

`GET /api/v1/research?limit=20&offset=0`

Authentication is optional. Anonymous callers receive only papers whose status
is `published`. `limit` defaults to 20 and must be between 1 and 100; `offset`
defaults to 0.

The response data has this shape:

```json
{
  "items": [
    {
      "id": "<uuid>",
      "author_id": "<uuid>",
      "title": "A research title",
      "abstract": "A short summary",
      "content": "The full research paper",
      "status": "published",
      "submitted_at": "<timestamp>",
      "under_review_at": "<timestamp>",
      "decided_at": "<timestamp>",
      "published_at": "<timestamp>",
      "elo_award": 25,
      "elo_awarded": true,
      "elo_awarded_at": "<timestamp>",
      "created_at": "<timestamp>",
      "updated_at": "<timestamp>"
    }
  ],
  "limit": 20,
  "offset": 0,
  "has_more": false
}
```

### Read a published paper

`GET /api/v1/research/{research_id}`

Authentication is optional for published papers. Anonymous requests for draft,
submitted, under-review, approved, or rejected papers return `404 NOT_FOUND`.
The response includes the publication timestamp and the completed public award
result when available. Reviewer identities and review payloads remain private.

Published papers whose award has not settled yet return `elo_awarded: false`,
`elo_award: null`, and `elo_awarded_at: null`. A completed award returns the
awarded Elo-point value and the settlement timestamp.

## Errors

Errors use the same envelope with an `error` object. Clients should branch on
the stable `code` rather than the message:

| Situation | HTTP status | Code |
| --- | ---: | --- |
| Missing/invalid session on author operation | 401 | `UNAUTHENTICATED` |
| Non-JSON authoring request | 415 | `INVALID_REQUEST` |
| Request body larger than 1 MiB | 413 | `VALIDATION_FAILED` |
| Invalid UUID or request body | 400/422 | `INVALID_REQUEST` / `VALIDATION_FAILED` |
| Unpublished paper not visible to caller | 404 | `NOT_FOUND` |
| Paper is no longer editable/submittable | 409 | `CONFLICT` |
