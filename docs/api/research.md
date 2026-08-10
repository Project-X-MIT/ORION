# Research API

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
The response intentionally omits reviewer identities, review payloads, and Elo
award bookkeeping.

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
