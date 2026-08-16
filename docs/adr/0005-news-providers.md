# ADR 0005: Normalized and licensed market-news providers

- Status: Accepted
- Date: 2026-08-09
- Owner: sudhanshu001122

## Context

Market-news providers return different schemas, URL forms, text encodings and
symbol conventions. Provider responses are untrusted input, and a provider
being reachable does not mean that ORION is allowed to store or redistribute
its content. The public feed also needs deterministic deduplication and a
bounded failure mode when a provider is unavailable.

The database already owns the durable records in `news_sources`,
`news_articles` and `news_ingestion_runs`. The domain contract must consume
completed rows from those tables without leaking provider-specific DTOs into
the API or feed.

## Decision

`crates/orion-domain/src/news.rs` defines version 1 of the normalized
contracts:

- `NewsSource` mirrors an admitted `news_sources` row and carries only stable
  source identity and URL metadata.
- `NewsArticle` mirrors a normalized `news_articles` row. Title, summary,
  content, attribution, category, symbols and URLs are represented in one
  stable shape for API and feed consumers.
- `NewsIngestionRun` mirrors `news_ingestion_runs` and uses a typed status so
  run outcomes cannot be treated as arbitrary strings.
- Under the completed DB-05 schema, the reconciled item-error count is derived
  as `articles_seen - articles_inserted`. `error_message` stores only the
  bounded run-level error code, so the worker does not create a competing
  schema or migration authority. A future repository query may expose the
  derived count as an alias without changing the persisted baseline.
- `ArticleLifecycle` and `ProviderHealth` make the article and provider state
  machines explicit. Articles advance only through
  `fetched -> normalized -> validated -> deduplicated -> persisted -> published`.
  Providers move from `healthy` to `degraded` or `circuit_open`, then through
  `probing` before returning to `healthy`.

Before persistence, the ingestion boundary calls `normalize` and then
`validate`:

- required text is trimmed and internal whitespace is canonicalized;
- source slugs are lowercase, ASCII, hyphen-separated identifiers;
- provider IDs are trimmed and cannot contain whitespace or control
  characters;
- article and source URLs are HTTPS-only, have no credentials, and have URL
  fragments removed;
- categories are lowercase and symbols are uppercase, restricted to market
  identifier characters, sorted and deduplicated;
- bounded field sizes and control-character checks reject oversized or unsafe
  input.

`ProviderRuntimePolicy` supplies bounded, framework-neutral runtime controls.
The worker owns enforcement; adapters do not create their own unbounded retry
or request loops. The default policy is:

- timeout: 10 seconds per request;
- rate limit: at most 60 requests per 60-second window, with a burst capacity
  of 5; retries count as requests;
- retry: 3 total attempts (initial request plus at most 2 retries), only for
  transport failures, timeouts, rate limits and upstream server failures, with
  250 ms then 500 ms exponential backoff capped at 4 seconds;
- circuit breaker: open after 5 consecutive retryable failures, remain open
  for at least 60 seconds, permit one probe, and require 2 successful probes
  to close.

Invalid payloads, authentication failures and license violations are never
retried. A provider `Retry-After` value may delay a retry, but the worker must
still honor the policy cap and source rate limit. Provider-specific settings
may tighten these bounds, but cannot increase timeout, request rate, burst,
attempt count or backoff, or reduce the circuit-open/probe safety window.
Circuit state is operational coordination owned by the worker; it is not
stored in the public article/source rows and Redis loss must not lose accepted
articles.

Freshness and failure signals are typed and safe to emit to observability and
feed consumers:

- `NewsFreshnessPolicy` uses `published_at` and the current UTC observation
  time. By default, an article is `fresh` for 15 minutes, `aging` through 1
  hour, `stale` through 24 hours, and `expired` afterward. Future-dated
  articles emit `future_dated` and remain hidden from the feed until their
  publication time;
- stale articles remain durable for audit and may be shown with a stale
  indicator; expired articles are excluded from the default feed but are not
  silently deleted;
- `NewsErrorSignal` contains only source/article IDs, a typed error category,
  UTC occurrence time and attempt number. It never carries raw provider
  responses, credentials or arbitrary error text;
- only `ProviderFailureKind::Transport`, `Timeout`, `RateLimited` and
  `UpstreamServer` signals are retryable. Normalization, validation,
  deduplication, persistence, authentication, license and circuit-open
  signals do not schedule another provider request.

The database remains authoritative. `NewsDeduplicationIdentity` makes the
database's stable identity order explicit:

1. `CanonicalUrl(normalized_article.url)` is always the primary key because
   `news_articles.url` is globally unique.
2. `SourceExternalId { source_id, external_id }` is an ordered fallback when
   the provider supplies a non-blank stable external ID, matching the
   `(source_id, external_id)` partial unique index.

URL sanitization lowercases the scheme and host, removes fragments and common
marketing parameters, rejects credentials and encoded control characters, and
requires HTTPS. Path and non-tracking query case/order are preserved. Adapters
must provide the provider's canonical article URL and must not use a redirect
URL as the identity. The existing database upsert and unique constraints must
be used inside the ingestion transaction. A retry reuses the same identity
candidates and cannot create a second row. The `source_id` is never omitted
from an external-ID key, so two providers may reuse the same external ID
safely.

All persisted domain timestamps are `DateTime<Utc>` and serialize as RFC 3339
UTC (`Z`). Ingestion must convert provider offsets to UTC before normalization;
naive or unparseable timestamps are rejected at the adapter boundary. Source
and article `created_at` values cannot be later than `updated_at`;
`ingested_at` cannot be later than `updated_at`; and an ingestion run's
`completed_at`, when present, cannot precede `started_at`. A run is `running`
only when it has no completion timestamp, and `completed`/`failed` runs must
have one. Articles with `published_at` in the future may be persisted for
later evaluation but cannot enter the feed until `published_at <= now` in
UTC. `news_ingestion_runs` records the attempt, but a failed or incomplete
run cannot publish an article.

License approval is an ingestion admission precondition. A provider may be
configured only when its agreement or public license permits the required
fetch, normalization, durable storage and feed redistribution, including any
required attribution. The existing `news_sources` table does not contain a
license column, so the domain contract does not infer licensing from a row's
existence; provider configuration owns the approval and attribution evidence.
`SourceLicenseApproval` is the auditable configuration record for that
decision. It must identify the source, name the license or agreement, link to
the evidence, explicitly allow feed redistribution, require
`SourceAndArticleLink` attribution, and have a current approval window.
Unapproved, expired, future-dated or ambiguous sources fail closed and are not
published. This ADR approves the admission policy; it does not claim that a
named publisher is licensed until its evidence is entered and reviewed.

The initial source set supplied for this issue is documented below as a
candidate register. These entries are not secrets and do not require an API
key. `NEWS_API_BASE_URL` is only a configuration slot; it is not approval for
whichever host is placed in that setting. Before enabling a source in
production, its configuration record must include, at minimum:

| Field | Required evidence |
| --- | --- |
| provider/source identity | Stable source ID and the exact production host |
| license or agreement | License/agreement name and a reviewable HTTPS evidence URL |
| permitted use | Explicit permission for fetching, normalization, durable storage and feed redistribution |
| attribution | Required source name and original article-link behavior |
| operational limits | Timeout, request/window, burst, retry and circuit-breaker values at or below `ProviderRuntimePolicy` bounds |
| approval window | Approver, approval timestamp, and optional expiry/review date |

Until such a record exists, ingestion must fail closed with a license or
configuration admission error and no article may be published. This prevents
an unreviewed provider from becoming an accidental production source and
keeps the legal decision outside the core domain code.

### Initial candidate source register

The feed URLs are kept at the provider-adapter/configuration boundary. A
directory or press page is recorded for source identity and attribution, but
must not be polled as if it were an RSS document. The worker must use the
concrete feed endpoint selected from that directory.

| Source | Feed endpoint or discovery page | Reuse evidence and attribution | Operational limit | Admission |
| --- | --- | --- | --- | --- |
| U.S. SEC press releases | `https://www.sec.gov/news/pressreleases.rss` | SEC government-created content; cite U.S. SEC and retain the original filing/release link | SEC maximum 10 requests/second; ORION uses the tighter default policy and declares a `User-Agent` | Candidate; Div registry entry required |
| U.S. SEC trading suspensions | `https://www.sec.gov/enforcement-litigation/trading-suspensions/rss` | Same SEC reuse policy and SEC attribution | Same SEC fair-access limit | Candidate; Div registry entry required |
| U.S. SEC structured filings | `https://www.sec.gov/Archives/edgar/usgaap.rss.xml`, `https://www.sec.gov/Archives/edgar/xbrl-rr.rss.xml`, `https://www.sec.gov/Archives/edgar/xbrl-inline.rss.xml`, or `https://www.sec.gov/Archives/edgar/xbrlrss.all.xml`; discovery page: `https://www.sec.gov/data-research/structured-data/structured-disclosure-rss-feeds` | EDGAR public filing content; cite U.S. SEC/EDGAR and retain the filing link | SEC maximum 10 requests/second; ORION uses the tighter default policy and declares a `User-Agent` | Candidate; select exact feed(s) before registry entry |
| European Central Bank press and market updates | `https://www.ecb.europa.eu/rss/press.html`, `https://www.ecb.europa.eu/rss/operations.html`, or `https://www.ecb.europa.eu/rss/yc.html`; discovery page: `https://www.ecb.europa.eu/home/html/rss.en.html` | ECB permits accurate reuse with ECB citation; exclude authored papers or third-party material requiring separate permission | No feed-specific limit is published on the directory; ORION imposes its default timeout, rate, retry and circuit policy | Candidate; Div registry entry required |
| GOV.UK business, economic and international updates | `https://www.gov.uk/search/news-and-communications.atom` | Open Government Licence v3.0 attribution; exclude logos and third-party material | No feed-specific limit is published; ORION imposes its default policy | Candidate; Div registry entry required |
| European Commission international and economic updates | RSS endpoint: `https://ec.europa.eu/commission/presscorner/api/rss`; discovery/attribution pages: `https://commission.europa.eu/news-and-media_en` and `https://commission.europa.eu/about/contact/press-services/press-room_en` | EU-owned editorial content is generally CC BY 4.0 with attribution and change indication; exclude third-party content, images and logos unless separately cleared | No feed-specific limit is published; ORION imposes its default policy | Candidate; Div registry entry and item-level rights filtering required |

The register is deliberately fail-closed: a candidate source is not a
production source until Div records the source ID, evidence URL, attribution,
operational policy, approver and approval window in the configuration registry.

Attribution and links follow fixed rules:

- every published article renders the admitted source name;
- every published article links to the article's normalized original `url`;
- the attribution link is HTTPS-only, contains no credentials or fragment, and
  is not replaced with a redirect URL or unrelated landing page; known tracking
  parameters are removed by URL sanitization;
- `source_url` is feed metadata and is not used as a substitute for the
  original article link;
- article content remains plain text and is escaped by the API/UI.

`NewsArticle::attribution` constructs this form from the normalized source,
article and active approval, preventing callers from supplying an arbitrary
attribution target.

Normalized article content is treated as plain text at the API/UI boundary;
the domain sanitizer rejects HTML-like markup, including in source and
article display fields, and encoded control characters in URLs; provider
adapters must extract plain text before normalization. Clients must still
escape it and must not render provider content as trusted HTML.
Secrets, credentials, cookies, raw provider payloads and unnecessary personal
data are excluded from the normalized contract and logs.

Provider contract tests are mandatory for every adapter. The shared contract
fixtures must cover missing required fields, malformed or naive timestamps,
offset-to-UTC conversion, unsafe HTML/URL input, normalization idempotence and
equivalent URL variants producing the same deduplication identity. A provider
DTO is deserialized and validated at its adapter boundary, then converted to
these provider-neutral types; provider-specific fields never cross that
boundary.

## Consequences

Feed consumers receive deterministic, provider-neutral records and can rely
on stable lifecycle and health states. Retries are safe when they reuse the
database deduplication keys and ingestion-run transaction. Provider outages
degrade availability for that provider without making Redis or an in-memory
cache authoritative.

Adding a persisted license or attribution field requires a forward-only
database migration, model/query changes and a new compatibility review. A
provider-specific field must not be added to the shared domain contract; it
belongs at the provider adapter boundary.
