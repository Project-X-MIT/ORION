# ADR 0007: Research Elo award handoff contract

- Status: Accepted
- Date: 2026-08-11
- Owners: Research/Phantom feature owner; Quiz/Elo feature owner; Div for the shared event registry

## Context

Phantom owns research evaluation and publication eligibility. The Elo owner
owns rating policy, calculation, and settlement. Phantom must therefore hand
off validated research facts without calculating or supplying an Elo delta.

The handoff is durable and may be delivered more than once. PostgreSQL's
outbox is the delivery boundary; the Elo consumer is responsible for claiming
the event and applying the award idempotently.

## Decision

Phantom emits the following versioned request:

```text
event_type:     orion.research.elo_award.requested
contract_version: 1
producer:       Phantom
consumer:       Elo settlement
```

The v1 JSON payload is:

```json
{
  "contract_version": 1,
  "paper_id": "uuid",
  "review_id": "uuid",
  "author_id": "uuid",
  "paper_status": "published",
  "rubric_version": 1,
  "evaluated_content_version": 7,
  "evaluation_score": 90,
  "recommendation": "approve",
  "idempotency_key": "research-paper:{paper_id}:review:{review_id}:elo-award"
}
```

### Field rules

| Field | Rule |
| --- | --- |
| `contract_version` | Must equal `1`; a breaking shape change requires a new version. |
| `paper_id` | Non-zero identity of the published research paper. |
| `review_id` | Non-zero identity of the approved review that produced the request. |
| `author_id` | Non-zero user identity receiving the award. |
| `paper_status` | Must equal `published`. |
| `rubric_version` | Positive, supported evaluation rubric version. |
| `evaluated_content_version` | Positive immutable content version used by the evaluation. |
| `evaluation_score` | Integer in the inclusive range `0–100`; it is a fact, not an Elo delta. |
| `recommendation` | Must equal `approve`. |
| `idempotency_key` | Must be stable and match `research-paper:{paper_id}:review:{review_id}:elo-award`. |

The payload must not contain `elo_delta`, `K`, `Sa`, expected score, current
ratings, or calculated before/after values. The Elo consumer maps the validated
research facts to the active research award policy, derives the positive award,
and completes the shared PostgreSQL rating transaction.

The resulting Elo calculation includes policy version `1` and validated source
metadata derived from this request: `source_type = research_review`,
`source_id = review_id`, and `source_version = contract_version`. Phantom's
request remains the source of facts; the Elo consumer owns the versioned
calculation output.

### Processing and idempotency

1. Phantom validates the published paper, approved review, rubric result, and
   content version before writing the outbox row.
2. The outbox row is written in the same transaction as the publication state
   change.
3. The Elo consumer validates the contract and claims the event before applying
   any effect.
4. Redelivery with the same event or idempotency key returns the existing
   settlement and must not create a second ledger entry or award.
5. A failed settlement leaves the event retryable and leaves current ratings,
   audit history, and the research award marker unchanged.

The rating ledger source is `research_review`. The paper and review identities
remain available for audit, while the idempotency key is the uniqueness key for
the approved review decision.

### Compatibility

Adding optional fields is compatible only when v1 consumers can ignore them.
Changing required fields, validation rules, or the meaning of the score
requires a new contract version and a migration window. The shared event
registry entry is proposed through Div, its single owner.

## Consequences

- Phantom cannot accidentally become a second Elo calculator.
- The Elo consumer receives enough provenance to audit the award without
  trusting a caller-supplied delta.
- The current producers must emit both `contract_version` and `review_id`.
- Research score-to-award mapping remains an Elo-policy decision owned by the
  Quiz/Elo feature, not by Phantom.
