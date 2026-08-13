# ADR 0005: Advanced Quiz actual-value calendar and timestamps

- Status: Proposed — pending product approval
- Date: 2026-08-10
- Owners: Advanced Quiz feature owner; Div for any migration ordering

## Context

Advanced Quiz predictions may depend on a market value that is not available
when the prediction is submitted. Evaluation therefore needs one deterministic
definition of the market session, the resolution horizon, and the timestamps
used by the `submitted -> pending -> scored -> settled` lifecycle.

The policy must also handle holidays, early closes, daylight-saving changes,
provider publication delay, and later corrections without changing the meaning
of an already-issued question or rating event.

## Decision

### Shared ELO scoring

Both quiz types use the same guarded ELO formula:

```text
point_delta = K * (Sa - Ea)
player_new_elo = clamp(player_elo + point_delta)
question_new_elo = clamp(question_elo - point_delta)
```

Basic uses the fixed `K = 20` and `Sa` equal to `1` for a correct answer or
`0` for an incorrect answer. Advanced derives `K` and `Sa` from its error
percentage zone table:

| Error | Zone | K | Sa | Effect |
| ---: | --- | ---: | ---: | --- |
| 0% | Win | 30 | 1 | Strong positive reward |
| 1% | Win | 27.5 | 1 | Positive reward |
| 2–3% | Win | 25 | 1 | Positive reward |
| 4–5% | Win | 18.28 | 1 | Positive reward |
| 6–8% | Win | 16.00 | 1 | Smaller positive reward |
| 9–10% | Neutral | 0 | 0 | No rating movement |
| 11–20% | Mild penalty | 15.00 | 0 | Small negative delta |
| 21–30% | Mild penalty | 25.00 | 0 | Medium negative delta |
| 31–50% | Mild penalty | 30.00 | 0 | Larger negative delta |
| 51%+ | Severe penalty | 35.00 | 0 | Maximum negative delta |

Neither mode uses a separate minimum/maximum marks range to determine rating
movement. Any future user-visible points system must be a separately specified
ledger and must not be confused with ELO `K`.

### Market calendar

1. Every market-backed Advanced question has an immutable, versioned
   `market_calendar_id`. The calendar identifies the market sessions, regular
   closes, holidays, and exceptional/early-close sessions used to resolve the
   question.
2. Calendar definitions are allowlisted application data. Business logic must
   not infer a calendar from the user's locale, server locale, weekday rules,
   or a fixed offset.
3. A question stores or references the calendar version selected when it is
   issued. Updating a calendar does not retroactively move the horizon of an
   existing question.
4. A horizon must resolve to a valid boundary in that calendar. A holiday,
   closed session, ambiguous local time, or daylight-saving gap is a rejected
   question configuration; the system must not silently roll it forward or
   backward.

### Timezone

1. UTC is the canonical storage, comparison, API, event, and audit timezone.
   Persisted timestamps use PostgreSQL `TIMESTAMPTZ` and domain values use
   `DateTime<Utc>`.
2. Calendar schedules are interpreted using an IANA timezone identifier held
   by the calendar definition. Abbreviations such as `EST`, local machine
   time, and fixed numeric offsets are not valid schedule configuration.
3. API timestamps must be RFC 3339 instants with an explicit offset. Inputs are
   normalized to UTC at the boundary; timestamps without an offset are
   rejected.
4. DST transitions are resolved by the calendar resolver. A nonexistent or
   ambiguous local schedule timestamp is invalid and requires a corrected
   question configuration rather than an implicit choice.

### Horizon and timestamp semantics

1. `horizon_at` is the UTC instant at which the observation window ends. It is
   calculated once from the question's calendar version and horizon boundary;
   it is not recalculated from the current calendar at settlement time.
2. A prediction is accepted only when its server-recorded
   `submitted_at < horizon_at`. The server/database receipt time is
   authoritative; client-provided timestamps are retained only as metadata.
3. The actual-value source must provide an authoritative value with an
   observation timestamp at or before `horizon_at` and a final/available
   timestamp at or after `horizon_at`. Provider timestamps are normalized to
   UTC and retained with the source reference and source version.
4. The lifecycle timestamps have these meanings and are written by the server:

   | Timestamp | Meaning |
   | --- | --- |
   | `submitted_at` | The server accepted the prediction before the horizon. |
   | `horizon_at` | The fixed end of the market observation window. |
   | `actual_observed_at` | When the source measured the value used for scoring. |
   | `actual_available_at` | When the source marked that value final and the server accepted it. |
   | `scored_at` | When the deterministic Advanced error/ELO calculation committed. |
   | `settled_at` | When the PostgreSQL transaction committed the attempt and rating ledger. |
   | `corrected_at` | When an approved correction was recorded, if one is required. |

5. Valid ordering is `submitted_at < horizon_at`,
   `actual_observed_at <= horizon_at <= actual_available_at <= scored_at <=
   settled_at`. A correction must occur after settlement and must create a
   compensating immutable ledger event; it must not rewrite the original event.
6. If the source is not final when the horizon is reached, the attempt remains
   `delayed` and is not scored. The expiry/grace deadline is an explicit
   question or calendar policy value, compared as a UTC instant. At expiry,
   the attempt becomes `expired` without a rating update unless a valid final
   value was accepted first.
7. All state-transition and settlement timestamps come from the database
   transaction clock. Retries reuse the existing lifecycle timestamps and
   cannot create a second score or settlement for the same attempt/question.

### Decimal precision, units, currency, and rounding

1. Predictions and actual values are exact decimal quantities. APIs represent
   them as decimal strings, the domain uses `rust_decimal::Decimal`, and the
   proposed PostgreSQL representation is a bounded `NUMERIC(38,18)`. Binary
   floating-point values are not accepted as the source of truth.
2. A question defines an immutable value contract: `unit_code`, optional
   uppercase ISO 4217 `currency_code`, and `scale`. The prediction and actual
   value must use the same contract. Values with more fractional digits than
   the question's scale are rejected; they are never silently rounded on
   ingestion. Trailing zeroes may be normalized, and negative zero is stored
   as zero.
3. Currency is required for monetary units and forbidden for dimensionless
   units such as a percentage. Percentages are expressed as percentage points
   (`2.50` means `2.50%`, not `0.025`). No implicit unit conversion, FX
   conversion, split adjustment, or quote-currency substitution is performed
   during scoring; the actual-value source must already provide the declared
   unit and currency.
4. The source's original decimal text, declared unit/currency, precision, and
   source version are retained for audit. A source value that cannot be
   represented by the question's exact decimal contract is unavailable for
   settlement and follows the delayed/expired path rather than being rounded
   into a different value.
5. Relative error is calculated exactly as
   `abs(predicted - actual) / abs(actual) * 100`. If the actual value is zero,
   equal zero prediction has `0%` error and any other prediction has `100%`
   error. The exact decimal relative error is retained; it is not replaced by
   the zone input.
6. The Advanced zone lookup receives the exact relative error rounded to the
   nearest integer using round-half-away-from-zero (`4.5 -> 5`; error is
   non-negative). The existing `51+` severe-penalty bucket handles every
   rounded value above 50. Values too large for the in-memory integer lookup
   are classified as severe without wrapping; the unrounded decimal remains
   the audit value.
7. ELO expected-score and raw-delta calculations do not round intermediate
   values. At the integer rating-ledger boundary, the raw delta is rounded to
   the nearest integer using the same half-away-from-zero rule, then the
   player and question guardrails are applied. The immutable event records the
   raw delta and the actually applied integer change separately when schema
   support is added; retries must reuse both values.

## Consequences

- Evaluation is reproducible across server locations and daylight-saving
  changes.
- Calendar data and provider metadata become part of the immutable evaluation
  context.
- The eventual migration manifest must add the approved calendar/source,
  horizon, actual-value, and lifecycle fields without editing an applied
  migration. Div owns migration ordering.
- Product approval is still required for the allowlisted actual-value source,
  supported calendar identifiers, and the expiry/grace duration.

## Alternatives rejected

- Using the user's or server's local timezone: produces different horizons for
  the same question.
- Recomputing a horizon from the latest calendar: makes already-issued
  predictions non-reproducible.
- Treating provider ingestion time as the market observation time: allows
  network delay to change the evaluated market value.
- Overwriting a settled result after a correction: destroys the rating audit
  trail and breaks idempotent settlement.
