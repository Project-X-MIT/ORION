# Quiz API

The machine-readable OpenAPI fragment for these routes is available at
[quiz.openapi.yaml](quiz.openapi.yaml).

All quiz endpoints require an active authenticated session cookie. Responses
use the shared versioned envelope:

```json
{
  "api_version": 1,
  "request_id": "<uuid>",
  "data": {}
}
```

## Basic question retrieval

`GET /api/v1/quiz/basic`

Optional query parameters are `limit` (default `20`, maximum `100`) and
`offset` (default `0`). The response contains active Basic Quiz questions in
stable database order:

```json
{
  "items": [
    {
      "id": "<question-uuid>",
      "category": "science",
      "question_text": "What is the chemical symbol for water?",
      "input_type": "mcq",
      "options": [
        {
          "id": "<option-uuid>",
          "option_text": "H2O",
          "position": 0
        }
      ]
    }
  ],
  "limit": 20,
  "offset": 0,
  "has_more": false
}
```

The answer key, question explanation, active flag, and question rating are
never returned by this endpoint. The response is marked `Cache-Control:
private, no-store`.

## Basic answer submission

`POST /api/v1/quiz/basic/attempts`

The request must use `Content-Type: application/json` and contain a caller-
generated `attempt_id`. Reusing the same attempt ID returns the already
committed result without applying another rating change; clients should bind an
attempt ID to one answer payload.
The authenticated user is taken from the session; a user ID in the body is not
accepted.

```json
{
  "attempt_id": "<attempt-uuid>",
  "answers": [
    {
      "question_id": "<question-uuid>",
      "option_id": "<option-uuid>"
    }
  ],
  "started_at": "2026-08-13T10:00:00Z",
  "completed_at": "2026-08-13T10:00:12Z"
}
```

`started_at` and `completed_at` are optional and default to the server's
receipt time. They must not be in the future. Each question may occur only
once, each answer must select one option, and a submission may contain at
most 100 answers. `selected_option_id` is accepted as a compatibility alias
for `option_id`. Any client-supplied `score`, `outcome`, `delta`, or `rating`
fields are ignored; the server derives all scoring and rating values from the
submitted options and authoritative database state.

Successful submissions return the settled attempt, the user's current rating,
and one result per answer:

```json
{
  "attempt": {
    "id": "<attempt-uuid>",
    "status": "completed",
    "total_questions": 1,
    "correct_answers": 1,
    "score": 100,
    "rating_before": 500,
    "rating_after": 510,
    "started_at": "2026-08-13T10:00:00Z",
    "completed_at": "2026-08-13T10:00:12Z"
  },
  "rating": {
    "rating": 510,
    "games_played": 1,
    "wins": 1,
    "losses": 0,
    "draws": 0
  },
  "answers": [
    {
      "question_id": "<question-uuid>",
      "correct": true,
      "rating_delta": 10
    }
  ]
}
```

The database transaction validates that every referenced question is active
and Basic, verifies the selected option against PostgreSQL's answer key, and
atomically writes the completed attempt, user/question ratings, immutable
rating events, and rating ledger entry. No route handler writes quiz or rating
tables directly.

## Advanced question retrieval

`GET /api/v1/quiz/advanced`

This authenticated endpoint uses the same `limit`, `offset`, response envelope,
private caching policy, and answer-key-safe question projection as Basic Quiz.
It returns only active Advanced questions and their selectable prediction
options; explanations, answer keys, and internal ratings are omitted.

## Advanced prediction submission

`POST /api/v1/quiz/advanced/attempts`

Advanced predictions support both option-shaped and exact numeric questions.
Numeric questions are returned with `input_type: "numeric"` and an empty
`options` array:

```json
{
  "attempt_id": "<attempt-uuid>",
  "predictions": [
    {
      "question_id": "<question-uuid>",
      "option_id": "<option-uuid>"
    }
  ]
}
```

`answers` is accepted as a compatibility alias for `predictions`, and
`selected_option_id` is accepted as an alias for `option_id`. The same
validation, authentication, timestamp, duplicate-attempt, and maximum-size
rules apply as for Basic submissions. Option predictions verify the question
and option type, then commit the completed attempt and shared user/question
rating update atomically. Numeric predictions are parsed as exact decimal
strings, validated against active Advanced questions with no options, written
to PostgreSQL `NUMERIC(38,18)`, and returned with `status: "pending"`. The
worker later obtains and validates the provider actual, then uses the existing
shared atomic scorer and `202608160001_advanced_actual_settlement.sql` to
complete the attempt. Client-supplied `score`, `outcome`, `delta`, or `rating`
fields are ignored and never used for settlement.

## Attempt/result retrieval

`GET /api/v1/quiz/attempts/{attempt_id}`

Returns the completed attempt result only when the attempt belongs to the
authenticated user. The response includes the persisted attempt summary,
current user rating, and immutable per-question rating results. Basic results
use `answers`; Advanced results use `predictions`:

```json
{
  "attempt": {
    "id": "<attempt-uuid>",
    "quiz_type": "basic",
    "status": "completed",
    "total_questions": 1,
    "correct_answers": 1,
    "score": 100,
    "rating_before": 500,
    "rating_after": 510,
    "started_at": "2026-08-13T10:00:00Z",
    "completed_at": "2026-08-13T10:00:12Z"
  },
  "rating": {
    "rating": 510,
    "games_played": 1,
    "wins": 1,
    "losses": 0,
    "draws": 0
  },
  "answers": [
    {
      "question_id": "<question-uuid>",
      "correct": true,
      "rating_delta": 10
    }
  ]
}
```

Invalid UUIDs return `400 INVALID_REQUEST`. Missing, foreign, or still-pending
attempts return the same `404 NOT_FOUND` response so ownership and lifecycle
state are not disclosed.

## Errors

Transport/domain validation failures and database-backed quiz validation
failures use the shared `422 VALIDATION_FAILED` response. Missing or expired
sessions use the shared `401 UNAUTHENTICATED` response. Database availability
failures use `503 SERVICE_UNAVAILABLE`, unexpected database failures use
`500 INTERNAL`, and database/driver details are not returned to the client.
