# ADR 0005: Research lifecycle and evaluation rubric

- Status: Accepted
- Date: 2026-08-09
- Owners: Phantom for research evaluation; Yash for Elo consumption; Div for
  shared-contract changes
- Product owner approval: Approved on 2026-08-09

## Context

The research persistence work already provides the authoritative
`research_papers` and `research_reviews` tables.  The domain layer needs a
framework-neutral contract that can be used by API, worker, and persistence
adapters without importing Axum, SQLx, or Redis.  It also needs to make
approval auditable and prevent a revision from changing the meaning of an
already decided paper.

## Decision

### Cross-feature dependencies

This contract depends on DIV-02's versioned shared-contract foundation:
`VersionedEvent`, `EventEnvelope`, the shared event registry, and the existing
`orion.rating.updated` contract. It also depends on the YASH-03 Elo consumer
contract for score-to-Elo policy and application. Research does not duplicate
either implementation or add a direct crate dependency on Yash's Elo code.

The YASH-03 consumer contract is not assumed to be merged by this change. Div
must register the request event and Yash must approve its final payload and
consumer behavior before production integration.

Before downstream crates consume these contracts, Div must expose the feature
module from the shared `crates/orion-domain/src/lib.rs` export surface. That
shared-owned file is intentionally not changed by the research feature.

### Persistence boundary

Research persistence uses the completed DB-04 implementation. The authoritative
storage and guards remain in
`crates/orion-db/migrations/202608070006_research.sql`, with its corresponding
models, queries, `ResearchRepository`, review transaction, and integration
tests under `crates/orion-db`. This contract adds no parallel tables,
repository, or persistence model; the separate content-immutability migration
is a forward-only proposal to Div. `orion-domain` remains framework-neutral and
provides only validation and versioned contracts over the completed DB-04
storage.

### Lifecycle

The only in-place transitions are:

```text
draft -> submitted -> under_review -> approved -> published
                                      \-> rejected
```

`submitted` is the author's hand-off.  `under_review` is the claimable review
state.  A persisted review with a matching recommendation is required for an
`approved` or `rejected` decision.  Only `approved` may transition to
`published`; publication and any downstream Elo award remain separate
concerns, with Elo calculation and application owned by Yash's consumer.

After `approved`, `rejected`, or `published`, the existing row and its review
decision are immutable.  A revision creates a new `research_papers` row in
`draft` and collects a new set of `research_reviews`.  The old row is retained
as the audit record and never moves back to draft or under review.

The domain validates non-nil paper and author IDs, non-empty title and content,
review identity, reviewer authorization, and review feedback before accepting
the corresponding operation.  An entity cannot move directly from
`under_review` to `approved` or `rejected`; those transitions require a
validated review through the review-backed transition method.  Publication
still requires the paper to already be approved.

### Domain entities

`ResearchPaper` owns the paper identity, author, content fields, and strongly
typed lifecycle status.  `ResearchReview` owns the review identity, paper and
reviewer references, structured evaluation, and reviewer comments.  A paper
entity applies transitions through the legal transition table; a revision
copies the content into a new paper identity at `draft` and leaves the decided
entity unchanged.

Only an identity with the `reviewer` role may submit a review.  The paper
author cannot review their own paper, and a review is valid only while the
paper is `under_review`.  A review must identify the same reviewer that is
being authorized; administrators do not silently inherit reviewer authority.

### Appeals, re-review, and content versions

An `appeal` is available only for a rejected paper, must be requested by the
author, and must include a non-empty reason.  A normal `revision` may be
requested by the author after `approved`, `rejected`, or `published`.  Both
forms require a new paper ID; neither reopens the source row or changes its
reviews.  The new row starts at `draft`, collects new reviews, and must pass
the complete lifecycle again.  An appeal is therefore a new review cycle, not
a second decision on the old row.

Content may be edited in place only while `draft`.  Once submitted, under
review, approved, rejected, or published, the content and decision record are
immutable.  A published version remains public while a revised draft is being
reviewed; only the revised row can later be published.

### Evaluation payload

The structured value stored in either table's `evaluation_result` JSONB column
uses rubric version 1:

```json
{
  "rubric_version": 1,
  "evaluated_content_version": 1,
  "scores": {
    "relevance": 0,
    "methodology": 0,
    "evidence": 0,
    "originality": 0,
    "clarity_and_reproducibility": 0
  },
  "overall_score": 0,
  "recommendation": "approve",
  "rationale": "...",
  "evidence": [
    {
      "reference": "Results section",
      "finding": "The reported result matches the described method."
    }
  ],
  "strengths": ["..."],
  "concerns": ["..."]
}
```

`rubric_version` and `evaluated_content_version` are mandatory inputs. The
rubric version must be supported, and the evaluated content version must be a
positive version identifier supplied by Phantom for the exact content under
review. Every dimension is an integer from 0 through 100. The overall score is the
weighted integer score using relevance 15%, methodology 25%, evidence 30%,
originality 15%, and clarity/reproducibility 15%; fractional remainders are
truncated.  `recommendation` is
explicit and canonicalized to `approve` or `reject`; the domain does not infer
a decision from a numeric threshold.  Rationale is required, and feedback
requires at least one traceable evidence item with a non-empty reference and
finding.  Both `strengths` and `concerns` require at least one non-empty item;
feedback arrays may not contain empty items.  The scalar `score` and
`recommendation`
columns in `research_reviews` remain query-friendly projections of this
payload.  A completed review must have a valid payload before it can be used
to decide a paper.

### Elo award handoff

Phantom, the research evaluation owner, sends Yash's Elo consumer a versioned
`orion.research.elo_award.requested` request only after the paper is
`published` and the evaluation recommendation is `approve`:

```json
{
  "paper_id": "...",
  "author_id": "...",
  "paper_status": "published",
  "rubric_version": 1,
  "evaluated_content_version": 1,
  "evaluation_score": 85,
  "recommendation": "approve",
  "idempotency_key": "research-paper:<paper_id>:elo-award"
}
```

Phantom owns the rubric, evidence, feedback, score, and recommendation. It
does not calculate an Elo delta. Yash owns the score-to-Elo policy, applies the
result through the shared atomic rating transaction, and emits the existing
`orion.rating.updated` event with reason `research_award` after the rating
commit. The paper ID is the source identity and the deterministic idempotency
key prevents retries from awarding Elo twice. The request is coordination
data, not a replacement for authoritative PostgreSQL publication and award
state.

The request type is `ResearchEloAwardRequestV1`.  Adding it to the shared event
registry and coordinating the consumer rollout are Div/Yash integration steps;
the research feature does not calculate, apply, or directly call the Elo
implementation.

## Versioning and migration

The forward-only migration proposal
`202608090013_research_content_immutability.sql` adds a database guard that
prevents direct content changes after draft. It is proposed to Div and does
not edit the completed DB-04 migration or any earlier migration history. Any
future schema change must follow the same new, ordered, forward-only process.
The proposal is additive and data-preserving; rollback must use a separately
approved forward compensating migration rather than removing the guard from
migration history.

The rubric and evaluated content versions are embedded in the JSON payload
because those values are already persisted in `evaluation_result` and may
outlive the code that wrote them. The Rust contract is named
`ResearchEvaluationV1`; readers must reject unknown rubric versions and missing
version fields rather than reinterpret them.

No `version` or `revision` columns are required for the lifecycle in this
change.  A new paper ID is the new version boundary, and the previous row plus
its reviews provide the audit trail.  If product requirements later need
lineage queries such as "show every revision of this paper," the research
feature owner will propose `root_paper_id` and an integer revision field to
Div in a separate forward-only migration.  That proposal is intentionally not
part of this contract.

## Consequences

The domain contract is deterministic, serializable, and independent of
database or transport implementations. Invalid transitions, incomplete rubric
payloads, unknown rubric versions, and post-decision content rewrites fail
before publication or award processing. Reviewers can still use the existing
compatibility spellings `approved` and `rejected` when reading old rows; new
writes serialize the canonical spellings.
