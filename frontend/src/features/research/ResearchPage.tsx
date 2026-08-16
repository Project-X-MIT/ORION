import { useEffect, useMemo, useState, type FormEvent } from "react";

import { LiveRegion } from "../../shared/accessibility/LiveRegion";
import { isApiClientError } from "../../shared/api/errors";
import { Form } from "../../shared/forms/Form";
import { NumberField } from "../../shared/forms/NumberField";
import { MarkdownEditor } from "../../shared/forms/MarkdownEditor";
import { SelectField } from "../../shared/forms/SelectField";
import { TextField } from "../../shared/forms/TextField";
import { useAuth } from "../../providers/AuthProvider";
import { Alert } from "../../shared/ui/Alert";
import { Badge } from "../../shared/ui/Badge";
import { Button } from "../../shared/ui/Button";
import { Card } from "../../shared/ui/Card";
import { Pagination } from "../../shared/ui/Pagination";
import {
  useOwnResearchDrafts,
  usePublishedResearch,
  useResearchPaper,
  useResearchReviews,
  useResearchReviewQueue,
  useSubmitResearchReview,
} from "./hooks";
import { ResearchEditor } from "./ResearchEditor";
import type {
  ResearchPaper,
  ResearchLifecycleStatus,
  ResearchReview,
  ResearchReviewInput,
  ResearchRubricScores,
} from "./types";

export type ResearchPageProps = {
  paperId?: string;
};

const REVIEW_WEIGHTS: Record<keyof ResearchRubricScores, number> = {
  relevance: 15,
  methodology: 25,
  evidence: 30,
  originality: 15,
  clarity_and_reproducibility: 15,
};

const REVIEW_LABELS: Record<keyof ResearchRubricScores, string> = {
  relevance: "Relevance",
  methodology: "Methodology",
  evidence: "Evidence",
  originality: "Originality",
  clarity_and_reproducibility: "Clarity and reproducibility",
};

const LIFECYCLE: Array<{ status: ResearchLifecycleStatus; label: string }> = [
  { status: "draft", label: "Draft" },
  { status: "submitted", label: "Submitted" },
  { status: "under_review", label: "Under review" },
  { status: "approved", label: "Approved" },
  { status: "published", label: "Published" },
  { status: "awarded", label: "Awarded" },
];

function statusLabel(status: string): string {
  if (status === "under_review") return "Under review";
  if (status === "awarded") return "Awarded";
  return status.charAt(0).toUpperCase() + status.slice(1);
}

function statusExplanation(status: string): string {
  switch (status) {
    case "draft":
      return "You can continue editing this draft or submit it for review.";
    case "submitted":
      return "Your paper is in the review queue.";
    case "under_review":
      return "A reviewer is evaluating your paper against the research rubric.";
    case "approved":
      return "Your paper was approved and is waiting for publication processing.";
    case "rejected":
      return "A reviewer requested changes. Your original paper is preserved; create a revision to try again.";
    case "published":
      return "Your paper is available in the public research catalogue.";
    case "awarded":
      return "Your published paper has also received its platform award.";
    default:
      return "The platform is processing this research paper.";
  }
}

function formatDate(value: string | null): string {
  if (!value) return "Not recorded";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Date unavailable";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date);
}

/**
 * Derive the displayed lifecycle stage from the API payload only. Approval,
 * publication, and awards are server-owned transitions; this helper never
 * invents or persists a new status in the browser.
 */
export function lifecycleStatus(paper: ResearchPaper): ResearchLifecycleStatus {
  return paper.status === "published" && paper.elo_awarded ? "awarded" : paper.status;
}

function formatAward(value: number): string {
  return `${value > 0 ? "+" : ""}${value}`;
}

function awardSummary(paper: ResearchPaper): string {
  if (paper.status === "published" && paper.elo_awarded && paper.elo_award !== null) {
    return `Awarded rating: ${formatAward(paper.elo_award)} Elo points`;
  }
  return paper.status === "published" ? "Awarded rating: pending" : "Awarded rating: not applicable";
}

function errorMessage(error: unknown): string {
  if (isApiClientError(error)) {
    return `${error.message}${error.requestId ? ` Reference: ${error.requestId}.` : "."}`;
  }
  return error instanceof Error ? error.message : "The research service is unavailable.";
}

function paperIdFromPathname(): string | undefined {
  const parts = window.location.pathname.split("/").filter(Boolean);
  if (parts[0] !== "research" || !parts[1]) return undefined;
  try {
    return decodeURIComponent(parts[1]);
  } catch {
    return parts[1];
  }
}

function ResearchStatusBadge({ status }: { status: string }) {
  return <Badge aria-label={`Status: ${statusLabel(status)}`}>{statusLabel(status)}</Badge>;
}

function ResearchDocument({ paper }: { paper: ResearchPaper }) {
  const paragraphs = paper.content.split(/\n\s*\n/).filter((paragraph) => paragraph.trim());
  return (
    <div aria-label="Research paper content">
      {paragraphs.length > 0 ? paragraphs.map((paragraph, index) => (
        <p key={`${paper.id}-paragraph-${index}`}>{paragraph}</p>
      )) : <p>{paper.content}</p>}
    </div>
  );
}

function StatusTimeline({ paper }: { paper: ResearchPaper }) {
  const currentLifecycleStatus = lifecycleStatus(paper);
  const currentIndex = paper.status === "rejected"
    ? LIFECYCLE.findIndex((item) => item.status === "under_review")
    : LIFECYCLE.findIndex((item) => item.status === currentLifecycleStatus);
  return (
    <section aria-labelledby="research-status-heading">
      <h2 id="research-status-heading">Research status</h2>
      <p>
        <strong><ResearchStatusBadge status={currentLifecycleStatus} /></strong>. {statusExplanation(currentLifecycleStatus)}
      </p>
      <ol aria-label="Research lifecycle">
        {LIFECYCLE.map((item, index) => {
          const complete = index <= currentIndex && paper.status !== "rejected";
            const current = item.status === currentLifecycleStatus || (paper.status === "rejected" && item.status === "under_review");
          return (
            <li key={item.status} aria-current={current ? "step" : undefined}>
              <span aria-hidden="true">{complete ? "✓" : "○"}</span>{" "}
              {item.label}{current && paper.status === "rejected" ? " — changes requested" : ""}
            </li>
          );
        })}
      </ol>
      <dl>
        <div><dt>Created</dt><dd>{formatDate(paper.created_at)}</dd></div>
        <div><dt>Submitted</dt><dd>{formatDate(paper.submitted_at)}</dd></div>
        <div><dt>Under review</dt><dd>{formatDate(paper.under_review_at)}</dd></div>
        <div><dt>Decision</dt><dd>{formatDate(paper.decided_at)}</dd></div>
        <div><dt>Published</dt><dd>{formatDate(paper.published_at)}</dd></div>
        <div><dt>Awarded</dt><dd>{formatDate(paper.elo_awarded_at)}</dd></div>
        <div><dt>Awarded rating</dt><dd>{awardSummary(paper)}</dd></div>
      </dl>
      {paper.status === "published" && !paper.elo_awarded && (
        <p role="status">Award processing is handled separately after publication.</p>
      )}
      {paper.status === "published" && paper.elo_awarded && paper.elo_award !== null && (
        <p role="status">This paper was awarded {formatAward(paper.elo_award)} Elo points.</p>
      )}
    </section>
  );
}

function reviewDecisionLabel(review: ResearchReview): string {
  return review.recommendation === "reject" || review.recommendation === "rejected"
    ? "Changes requested"
    : review.recommendation === "approve" || review.recommendation === "approved"
      ? "Approved"
      : review.recommendation;
}

function ReviewFeedback({
  paper,
  reviews,
}: {
  paper: ResearchPaper;
  reviews: ReturnType<typeof useResearchReviews>;
}) {
  return (
    <section aria-labelledby="research-feedback-heading">
      <h2 id="research-feedback-heading">Review status and feedback</h2>
      {reviews.isPending && <p aria-live="polite">Loading review feedback…</p>}
      {reviews.error && (
        <Alert>
          <p>{errorMessage(reviews.error)}</p>
          <Button type="button" onClick={() => void reviews.refetch()}>Retry loading feedback</Button>
        </Alert>
      )}
      {!reviews.isPending && !reviews.error && reviews.data?.reviews.length === 0 && (
        <p>
          {paper.status === "submitted" || paper.status === "under_review"
            ? "Your paper is in review. Written feedback will appear here after a decision."
            : paper.status === "rejected"
              ? "This paper was rejected, but no written feedback was returned."
              : "No review feedback is available for this paper yet."}
        </p>
      )}
      {reviews.data?.reviews.map((review, index) => {
        const rejected = review.recommendation === "reject" || review.recommendation === "rejected";
        return (
          <Card as="article" key={`${review.reviewed_at}-${index}`} aria-labelledby={`research-review-${index}`}>
            <h3 id={`research-review-${index}`}>Review decision: {reviewDecisionLabel(review)}</h3>
            <p>Reviewed {formatDate(review.reviewed_at)}</p>
            {rejected && (
              <Alert title="Changes requested."> Review the feedback below before creating a revision.</Alert>
            )}
            {review.score !== null && <p>Review score: {review.score}/100</p>}
            {review.comments && (
              <section aria-labelledby={`research-review-comments-${index}`}>
                <h4 id={`research-review-comments-${index}`}>Reviewer feedback</h4>
                <p>{review.comments}</p>
              </section>
            )}
            {review.evaluation && (
              <section aria-labelledby={`research-review-evaluation-${index}`}>
                <h4 id={`research-review-evaluation-${index}`}>Evaluation details</h4>
                <p>{review.evaluation.rationale}</p>
                <h5>Strengths</h5>
                <ul>{review.evaluation.strengths.map((strength) => <li key={strength}>{strength}</li>)}</ul>
                <h5>Concerns</h5>
                <ul>{review.evaluation.concerns.map((concern) => <li key={concern}>{concern}</li>)}</ul>
                {review.evaluation.evidence.length > 0 && (
                  <>
                    <h5>Evidence</h5>
                    <dl>
                      {review.evaluation.evidence.map((item, evidenceIndex) => (
                        <div key={`${item.reference}-${evidenceIndex}`}>
                          <dt>{item.reference}</dt>
                          <dd>{item.finding}</dd>
                        </div>
                      ))}
                    </dl>
                  </>
                )}
              </section>
            )}
            {!review.comments && !review.evaluation && <p>No written feedback was included.</p>}
          </Card>
        );
      })}
    </section>
  );
}

function PaperReader({ paper }: { paper: ResearchPaper }) {
  const published = paper.status === "published";
  return (
    <Card as="article" aria-labelledby={`research-paper-${paper.id}`}>
      <header>
        <p><ResearchStatusBadge status={lifecycleStatus(paper)} /></p>
        <h2 id={`research-paper-${paper.id}`}>{paper.title}</h2>
        <p>{paper.published_at ? `Published ${formatDate(paper.published_at)}` : `Status: ${statusLabel(paper.status)}`}</p>
      </header>
      {published && (
        <section aria-labelledby={`research-publication-${paper.id}`}>
          <h3 id={`research-publication-${paper.id}`}>Publication details</h3>
          <dl>
            <div><dt>Published</dt><dd>{formatDate(paper.published_at)}</dd></div>
            <div><dt>Awarded rating</dt><dd>{awardSummary(paper)}</dd></div>
            {paper.elo_awarded_at && (
              <div><dt>Awarded</dt><dd>{formatDate(paper.elo_awarded_at)}</dd></div>
            )}
          </dl>
        </section>
      )}
      {paper.abstract && (
        <section aria-labelledby={`research-abstract-${paper.id}`}>
          <h3 id={`research-abstract-${paper.id}`}>Abstract</h3>
          <p>{paper.abstract}</p>
        </section>
      )}
      <section aria-labelledby={`research-content-${paper.id}`}>
        <h3 id={`research-content-${paper.id}`}>Paper</h3>
        <ResearchDocument paper={paper} />
      </section>
    </Card>
  );
}

function PublicCatalogue() {
  const [offset, setOffset] = useState(0);
  const page = usePublishedResearch({ limit: 10, offset });

  return (
    <section aria-labelledby="published-research-heading">
      <h2 id="published-research-heading">Published research</h2>
      <p>Read research that has completed the ORION review process.</p>
      {page.isPending && <LiveRegion>Loading published research…</LiveRegion>}
      {page.error && (
        <Alert>
          <p>{errorMessage(page.error)}</p>
          <Button type="button" onClick={() => void page.refetch()}>Retry loading research</Button>
        </Alert>
      )}
      {page.data && page.data.items.length === 0 && <p>No published research is available yet.</p>}
      {page.data && page.data.items.length > 0 && (
        <>
          <ul aria-label="Published research papers">
            {page.data.items.map((paper) => (
              <li key={paper.id}>
                <article>
                  <h3><a href={`/research/${encodeURIComponent(paper.id)}`}>{paper.title}</a></h3>
                  <p>{paper.abstract || "No abstract was provided."}</p>
                  <p>Published {formatDate(paper.published_at)}. {awardSummary(paper)}.</p>
                </article>
              </li>
            ))}
          </ul>
          <Pagination
            label="Published research pages"
            page={Math.floor(offset / 10) + 1}
            hasPrevious={offset > 0}
            hasNext={page.data.has_more}
            busy={page.isFetching}
            onPrevious={() => setOffset(Math.max(0, offset - 10))}
            onNext={() => setOffset(offset + 10)}
          />
        </>
      )}
    </section>
  );
}

function ReviewForm({ paper, onCompleted }: { paper: ResearchPaper; onCompleted?: (paper: ResearchPaper) => void }) {
  const mutation = useSubmitResearchReview();
  const [scores, setScores] = useState<ResearchRubricScores>({
    relevance: 0,
    methodology: 0,
    evidence: 0,
    originality: 0,
    clarity_and_reproducibility: 0,
  });
  const [recommendation, setRecommendation] = useState<"approve" | "reject">("approve");
  const [rationale, setRationale] = useState("");
  const [reference, setReference] = useState("");
  const [finding, setFinding] = useState("");
  const [strengths, setStrengths] = useState("");
  const [concerns, setConcerns] = useState("");
  const [comments, setComments] = useState("");
  const [contentVersion, setContentVersion] = useState("1");
  const [completed, setCompleted] = useState<ResearchPaper | null>(null);

  const overallScore = useMemo(() => {
    const weighted = (Object.keys(REVIEW_WEIGHTS) as Array<keyof ResearchRubricScores>)
      .reduce((total, key) => total + scores[key] * REVIEW_WEIGHTS[key], 0);
    return Math.floor(weighted / 100);
  }, [scores]);

  function setScore(field: keyof ResearchRubricScores, value: string) {
    const parsed = Number(value);
    setScores((current) => ({
      ...current,
      [field]: Number.isFinite(parsed) ? Math.min(100, Math.max(0, parsed)) : 0,
    }));
  }

  function lines(value: string): string[] {
    return value.split("\n").map((line) => line.trim()).filter(Boolean);
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input: ResearchReviewInput = {
      score: overallScore,
      recommendation,
      comments: comments.trim() || undefined,
      evaluation: {
        rubric_version: 1,
        evaluated_content_version: Math.max(1, Number(contentVersion) || 1),
        scores,
        overall_score: overallScore,
        recommendation,
        rationale: rationale.trim(),
        evidence: [{ reference: reference.trim(), finding: finding.trim() }],
        strengths: lines(strengths),
        concerns: lines(concerns),
      },
    };
    const nextPaper = await mutation.mutateAsync({ id: paper.id, input });
    setCompleted(nextPaper);
    onCompleted?.(nextPaper);
  }

  if (completed) {
    return (
      <p role="status" aria-live="polite">
        Review submitted. The paper is now <ResearchStatusBadge status={completed.status} />.
      </p>
    );
  }

  return (
    <Form onSubmit={submit} aria-labelledby={`review-form-${paper.id}`}>
      <h4 id={`review-form-${paper.id}`}>Complete review</h4>
      {mutation.error && (
        <Alert>
          <p>{errorMessage(mutation.error)}</p>
          <Button type="button" onClick={() => mutation.reset()}>Dismiss error</Button>
        </Alert>
      )}
      <fieldset disabled={mutation.isPending}>
        <legend>Rubric scores</legend>
        {(Object.keys(REVIEW_WEIGHTS) as Array<keyof ResearchRubricScores>).map((field) => (
          <NumberField
            key={field}
            id={`review-${paper.id}-${field}`}
            label={`${REVIEW_LABELS[field]} (${REVIEW_WEIGHTS[field]}% weight)`}
            description="Use a whole-number score from 0 to 100."
            min={0}
            max={100}
            step={1}
            value={scores[field]}
            onChange={(event) => setScore(field, event.target.value)}
            required
          />
        ))}
      </fieldset>
      <LiveRegion><strong>Overall score: {overallScore}/100</strong></LiveRegion>
      <SelectField
        id={`review-${paper.id}-recommendation`}
        label="Recommendation"
        value={recommendation}
        onChange={(event) => setRecommendation(event.target.value as "approve" | "reject")}
        options={[
          { value: "approve", label: "Approve" },
          { value: "reject", label: "Request changes" },
        ]}
        required
      />
      <NumberField
        id={`review-${paper.id}-version`}
        label="Evaluated content version"
        min={1}
        step={1}
        value={contentVersion}
        onChange={(event) => setContentVersion(event.target.value)}
        required
      />
      <MarkdownEditor
        id={`review-${paper.id}-rationale`}
        label="Rationale"
        value={rationale}
        onChange={(event) => setRationale(event.target.value)}
        rows={5}
        required
      />
      <fieldset>
        <legend>Evidence</legend>
        <TextField
          id={`review-${paper.id}-reference`}
          label="Reference"
          value={reference}
          onChange={(event) => setReference(event.target.value)}
          required
        />
        <MarkdownEditor
          id={`review-${paper.id}-finding`}
          label="Finding"
          value={finding}
          onChange={(event) => setFinding(event.target.value)}
          rows={3}
          required
        />
      </fieldset>
      <MarkdownEditor
        id={`review-${paper.id}-strengths`}
        label="Strengths (one per line)"
        value={strengths}
        onChange={(event) => setStrengths(event.target.value)}
        rows={4}
        required
      />
      <MarkdownEditor
        id={`review-${paper.id}-concerns`}
        label="Concerns (one per line)"
        value={concerns}
        onChange={(event) => setConcerns(event.target.value)}
        rows={4}
        required
      />
      <TextField
        id={`review-${paper.id}-comments`}
        label="Reviewer comments (optional)"
        value={comments}
        onChange={(event) => setComments(event.target.value)}
        rows={4}
        multiline
      />
      <Button type="submit" disabled={mutation.isPending}>
        {mutation.isPending ? "Submitting review…" : "Submit review"}
      </Button>
    </Form>
  );
}

function ReviewerQueue() {
  const queue = useResearchReviewQueue({ limit: 20, offset: 0 });
  return (
    <section aria-labelledby="research-review-queue-heading">
      <h2 id="research-review-queue-heading">Reviewer queue</h2>
      <p>Submitted and in-progress papers remain visible so interrupted reviews can be resumed.</p>
      {queue.isPending && <LiveRegion>Loading review queue…</LiveRegion>}
      {queue.error && (
        <Alert>
          <p>{errorMessage(queue.error)}</p>
          <Button type="button" onClick={() => void queue.refetch()}>Retry review queue</Button>
        </Alert>
      )}
      {queue.data?.items.length === 0 && <p>The review queue is empty.</p>}
      {queue.data && queue.data.items.length > 0 && (
        <ul aria-label="Research papers awaiting review">
          {queue.data.items.map((paper) => (
            <li key={paper.id}>
              <Card as="article">
                <h3>{paper.title}</h3>
                <p><ResearchStatusBadge status={paper.status} />. Submitted {formatDate(paper.submitted_at)}.</p>
                <details>
                  <summary>Read paper and complete review</summary>
                  {paper.abstract && <p><strong>Abstract:</strong> {paper.abstract}</p>}
                  <ResearchDocument paper={paper} />
                  <ReviewForm paper={paper} onCompleted={() => void queue.refetch()} />
                </details>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function ResearchPage({ paperId = paperIdFromPathname() }: ResearchPageProps) {
  const { status: authStatus, user } = useAuth();
  const authenticated = authStatus === "authenticated" && Boolean(user);
  const drafts = useOwnResearchDrafts(undefined, authenticated);
  const selectedPaper = useResearchPaper(paperId);
  const [editorPaper, setEditorPaper] = useState<ResearchPaper | undefined>();
  const [revisionSource, setRevisionSource] = useState<ResearchPaper | undefined>();

  useEffect(() => {
    setEditorPaper(undefined);
    setRevisionSource(undefined);
  }, [paperId]);

  const ownSelectedPaper = Boolean(
    selectedPaper.data && user && selectedPaper.data.author_id === user.id,
  );
  const authorPaper = ownSelectedPaper ? selectedPaper.data : undefined;
  const immutableAuthorPaper = paperId && authorPaper &&
    authorPaper.status !== "draft" && authorPaper.status !== "published"
    ? authorPaper
    : undefined;
  const authorReviews = useResearchReviews(authorPaper?.id);

  useEffect(() => {
    if (authorPaper?.status === "draft") setEditorPaper(authorPaper);
  }, [authorPaper?.id, authorPaper?.status, authorPaper?.updated_at]);

  function handleSaved(paper: ResearchPaper) {
    setEditorPaper(paper);
  }

  function handleSubmitted(paper: ResearchPaper) {
    setEditorPaper(paper);
  }

  return (
    <main>
      <header>
        <h1>ORION research</h1>
        <p>Write, review, and share research through a transparent publication lifecycle.</p>
      </header>

      {selectedPaper.isPending && <LiveRegion>Loading research paper…</LiveRegion>}
      {selectedPaper.error && (
        <Alert>
          <p>{errorMessage(selectedPaper.error)}</p>
          <Button type="button" onClick={() => void selectedPaper.refetch()}>Retry loading paper</Button>
        </Alert>
      )}
      {selectedPaper.data && (
        <>
          {authorPaper && <StatusTimeline paper={authorPaper} />}
          {authorPaper && <ReviewFeedback paper={authorPaper} reviews={authorReviews} />}
          {selectedPaper.data.status === "published" && <PaperReader paper={selectedPaper.data} />}
          {authorPaper && authorPaper.status !== "published" && (
            <section aria-labelledby="research-private-preview-heading">
              <h2 id="research-private-preview-heading">Private preview</h2>
              <PaperReader paper={authorPaper} />
            </section>
          )}
        </>
      )}

      {authenticated && (
        <section aria-labelledby="research-author-heading">
          <h2 id="research-author-heading">Author workspace</h2>
          <p>Save a draft first; submission moves it into the reviewer queue and locks its content.</p>
          {drafts.isPending && <LiveRegion>Loading your drafts…</LiveRegion>}
          {drafts.error && (
            <Alert>
              <p>{errorMessage(drafts.error)}</p>
              <Button type="button" onClick={() => void drafts.refetch()}>Retry loading drafts</Button>
            </Alert>
          )}
          {drafts.data?.items.length === 0 && <p>You do not have any saved drafts yet.</p>}
          {drafts.data && drafts.data.items.length > 0 && (
            <ul aria-label="Your research drafts">
              {drafts.data.items.map((draft) => (
                <li key={draft.id}>
                  <Button type="button" onClick={() => setEditorPaper(draft)}>
                    Edit {draft.title}
                  </Button>
                  <span> — last updated {formatDate(draft.updated_at)}</span>
                </li>
              ))}
            </ul>
          )}
          <ResearchEditor
            paper={revisionSource ?? immutableAuthorPaper ?? editorPaper}
            revisionSourceId={revisionSource?.id}
            onSaved={handleSaved}
            onSubmitted={handleSubmitted}
            onCancel={() => {
              setRevisionSource(undefined);
              setEditorPaper(undefined);
            }}
            onStartRevision={immutableAuthorPaper?.status === "rejected"
              ? () => setRevisionSource(immutableAuthorPaper)
              : undefined}
          />
        </section>
      )}

      {authenticated && user?.role === "reviewer" && <ReviewerQueue />}
      {!authenticated && (
        <p>
          <a href={`/login?returnTo=${encodeURIComponent(window.location.pathname)}`}>Sign in</a>{" "}
          to write and submit your own research.
        </p>
      )}
      <PublicCatalogue />
    </main>
  );
}
