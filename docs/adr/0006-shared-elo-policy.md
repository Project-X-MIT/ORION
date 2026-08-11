# ADR 0006: Shared Elo policy

- Status: Accepted
- Date: 2026-08-11
- Owners: Quiz/Elo feature owner; Div for shared contract and migration ordering

## Context

ORION currently has Elo behavior in both the domain scoring module and the
database settlement transaction. Those paths must agree on the baseline,
rating ranges, K-factor selection, rounding, and handling of a negative
rating change. Otherwise the same business outcome can produce different
ratings depending on which feature submitted it.

This ADR defines version 1 of the shared numeric policy. The policy is applied
by the pure domain calculation and its result is then persisted by the
completed PostgreSQL settlement transaction. It does not authorize direct
database writes from feature code.

## Decision

### Policy lifecycle

This policy is version `1` and is approved for implementation. Its lifecycle is
`draft -> approved/versioned -> active -> deprecated`. A deprecated policy is
never used for new settlements, but its rules remain available for interpreting
historical events.

### Initial rating

An uninitialized user or question starts at Elo `500`.

The initial value is applied only when the corresponding rating row is first
created. It is not re-applied during retries or after a row already has rating
history.

### Rating bounds

The domain applies separate inclusive bounds:

| Rating | Minimum | Maximum |
| --- | ---: | ---: |
| User/player | 100 | 3000 |
| Question | 100 | 2400 |

Inputs outside these ranges are invalid for a new settlement. Calculated
outputs are bounded at the same limits as a final guardrail. A bound may reduce
the actually applied change; the calculation must not overflow the bound.

The existing database range of `1–4000` is broader than this policy and does
not change the domain decision. Database constraints may be tightened later by
a new forward-only migration; an applied migration must not be edited.

### Expected score and K-factor

All user-versus-question updates use one guarded Elo formula:

```text
Ea = 1 / (1 + 10 ^ ((question_elo - player_elo) / 400))
raw_delta = K * (Sa - Ea)
```

`K` is selected by the active versioned policy and is not an arbitrary value
supplied by a caller.

- Basic Quiz uses fixed `K = 20`.
- Advanced Quiz uses the following deterministic zone table for its `K` and
  `Sa` values:

  | Error | Zone | K | Sa |
  | ---: | --- | ---: | ---: |
  | 0% | Win | 30 | 1 |
  | 1% | Win | 27.5 | 1 |
  | 2–3% | Win | 25 | 1 |
  | 4–5% | Win | 18.28 | 1 |
  | 6–8% | Win | 16.00 | 1 |
  | 9–10% | Neutral | 0 | 0 |
  | 11–20% | Mild penalty | 15.00 | 0 |
  | 21–30% | Mild penalty | 25.00 | 0 |
  | 31–50% | Mild penalty | 30.00 | 0 |
  | 51%+ | Severe penalty | 35.00 | 0 |
- Research has no active Elo calculation in policy version `1`. A future
  research policy must use this engine's versioned settlement input; research
  callers may not introduce an arbitrary direct Elo award.

### Reward-to-policy input mapping

Callers provide a business outcome, not a raw `K` or `Sa` value. The active
policy derives the formula inputs as follows:

| Feature | Business input | Derived policy inputs |
| --- | --- | --- |
| Basic Quiz | Answer is correct | `K = 20`, `Sa = 1` |
| Basic Quiz | Answer is incorrect | `K = 20`, `Sa = 0` |
| Advanced Quiz | Prediction and actual value | Calculate exact relative error, round it half-away-from-zero, then select the zone's `K` and `Sa` from the table above |

For Advanced Quiz, an exact prediction (`0%` error) maps to `K = 30`,
`Sa = 1`; a `9–10%` error maps to the neutral input `K = 0`, `Sa = 0`; and
an error of `51%` or more maps to `K = 35`, `Sa = 0`.

The settlement layer must persist the derived `K`, `Sa`, zone, and error
metadata. It must not accept caller-supplied values that bypass this mapping.

The formula is zero-sum before independent player and question bounds are
applied:

```text
player_after = player_before + raw_delta
question_after = question_before - raw_delta
```

### Rounding

Intermediate expected scores and raw deltas are not rounded. At the integer
rating-ledger boundary, the raw delta is rounded to the nearest integer using
round-half-away-from-zero (`4.5 -> 5`, `-4.5 -> -5`). The integer delta is
then applied and the separate player/question bounds are enforced.

If the rounded delta is zero, both ratings remain unchanged, although the
settlement may still be recorded as an accepted zero-movement event when the
owning transaction requires an audit row.

### Negative deltas

Negative deltas are valid Elo results and must not be discarded, converted to
absolute values, or treated as errors. They occur when the actual score is
below the expected score.

- A negative player delta decreases the player rating.
- The corresponding question delta is positive.
- The question and player changes remain inverse before bounds are applied.
- A rating can never pass its policy minimum.

Research award direction, score mapping, and correction semantics are not
defined by policy version `1`. Those rules require a separate approved
research Elo policy before any research rating settlement is activated.

### Validation and settlement boundary

The domain validates all inputs before calculating a delta. It returns a
versioned, deterministic settlement result containing the policy version,
validated source metadata, before ratings, expected score, raw delta, rounded
delta, outcome, and bounded after ratings.

Every Elo result carries the following immutable source metadata:

| Field | Rule |
| --- | --- |
| `source_type` | Required non-blank source category, such as `quiz_attempt` or `advanced_actual_value`. |
| `source_id` | Required non-blank stable source identity used for audit and idempotency. |
| `source_version` | Required non-blank source schema/content version. |

The domain trims surrounding whitespace and rejects missing metadata before a
source-backed result can be produced. JSON deserialization applies the same
validation. Source-aware calculation entry points copy the validated metadata
into the output; convenience calculations use the explicit validated
`domain_calculation/unattributed/1` provenance marker. Settlement consumers
must persist the policy version and source metadata with the rating event.

The PostgreSQL transaction remains responsible for locking the current
`user_ratings` and `question_ratings` rows, applying the result atomically,
writing `rating_events` and the append-only ledger, and making retries
idempotent.

Quiz callers are covered by the shared Elo contract tests. Research currently
has no approved Elo policy or calculation caller; its separate integration
tests cover only the versioned handoff facts and idempotent outbox behavior.
When a research Elo policy is designed, its consumer must call this same
calculation rather than introduce a second formula.

## Consequences

- Quiz settlement has one approved numeric policy; Research has no active Elo
  calculation until its policy is designed and approved.
- Existing Basic behavior is intentionally superseded by the approved `500`
  initial rating and `K = 20` policy.
- Advanced uses the approved deterministic zone-based K policy recorded in ADR
  0005.
- Domain implementation must use separate player and question bounds rather
  than a shared `1–4000` clamp.
- The existing database constraints remain permissive until a later
  forward-only migration tightens them. This is safe only while every write
  path uses the shared domain policy and transaction.
- Research publication no longer accepts an arbitrary direct award; it emits
  the versioned settlement request defined by ADR 0007. The Elo consumer owns
  the policy-generated settlement and its idempotent rating transaction.

## Alternatives rejected

- A single `1–4000` bound for both users and questions: it does not express the
  approved player and question limits.
- Rounding intermediate formula values: it makes results depend on evaluation
  order and loses precision.
- Rejecting every negative delta: it prevents expected-score correction and
  makes Elo non-deterministic for incorrect or underperforming outcomes.
- Applying arbitrary research awards directly: it creates a second rating
  policy outside the shared calculation and weakens auditability.
