import { useEffect, useRef, useState, type FormEvent } from "react";

import { LiveRegion } from "../../shared/accessibility/LiveRegion";
import { isApiClientError } from "../../shared/api/errors";
import { VisuallyHidden } from "../../shared/accessibility/VisuallyHidden";
import { Form } from "../../shared/forms/Form";
import { MarkdownEditor } from "../../shared/forms/MarkdownEditor";
import { TextField } from "../../shared/forms/TextField";
import { Alert } from "../../shared/ui/Alert";
import { Button } from "../../shared/ui/Button";
import { Modal } from "../../shared/ui/Modal";
import {
  useCreateResearchDraft,
  useCreateResearchRevision,
  useSubmitResearchPaper,
  useUpdateResearchDraft,
} from "./hooks";
import type { ResearchDraftInput, ResearchPaper } from "./types";

export type ResearchEditorProps = {
  paper?: ResearchPaper;
  revisionSourceId?: string;
  onSaved?: (paper: ResearchPaper) => void;
  onSubmitted?: (paper: ResearchPaper) => void;
  onCancel?: () => void;
  onStartRevision?: () => void;
};

type EditorValues = ResearchDraftInput;
type DraftField = keyof EditorValues;
type ValidationErrors = Partial<Record<DraftField, string>>;
type RetryAction = "save" | "prepare-submit" | "submit";

const EMPTY_VALUES: EditorValues = { title: "", abstract: "", content: "" };
const MAX_LENGTHS: Record<DraftField, number> = {
  title: 200,
  abstract: 5_000,
  content: 500_000,
};

function characterCount(value: string): number {
  return [...value].length;
}

function normalizeText(value: string): string {
  return value.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

function containsDisallowedText(value: string): boolean {
  return (
    [...value].some((character) => {
      const code = character.charCodeAt(0);
      return (code <= 8 || code === 11 || code === 12 || (code >= 14 && code <= 31)) ||
        (code >= 127 && code <= 159);
    }) ||
    /<(?=[A-Za-z/!?])/u.test(value) ||
    ["javascript:", "vbscript:", "data:", "file:"].some((scheme) =>
      value.toLowerCase().includes(scheme),
    )
  );
}

export function validateResearchDraft(values: EditorValues): ValidationErrors {
  const errors: ValidationErrors = {};
  const requiredFields: DraftField[] = ["title", "content"];

  for (const field of Object.keys(MAX_LENGTHS) as DraftField[]) {
    const normalized = normalizeText(values[field]);
    if (requiredFields.includes(field) && normalized.trim().length === 0) {
      errors[field] = `${field === "title" ? "Title" : "Paper content"} is required.`;
      continue;
    }
    if (characterCount(normalized.trim()) > MAX_LENGTHS[field]) {
      errors[field] = `${field === "title" ? "Title" : field === "abstract" ? "Abstract" : "Paper content"} must be ${MAX_LENGTHS[field].toLocaleString()} characters or fewer.`;
      continue;
    }
    if (containsDisallowedText(normalized)) {
      errors[field] = "Use plain text only. Markup, unsafe URL schemes, and control characters are not allowed.";
    }
  }

  return errors;
}

function requestErrorMessage(error: unknown): string {
  if (isApiClientError(error)) {
    const message = error.message.replace(/[.\s]+$/u, "");
    const requestId = error.requestId ? ` Reference: ${error.requestId}.` : "";
    return `${message}.${requestId}`;
  }
  return error instanceof Error ? error.message : "The research request failed. Try again.";
}

function valuesFromPaper(paper?: ResearchPaper): EditorValues {
  if (!paper) return EMPTY_VALUES;
  return { title: paper.title, abstract: paper.abstract, content: paper.content };
}

function statusLabel(status: string): string {
  return status === "under_review"
    ? "under review"
    : status.replace("_", " ");
}

function formatDate(value: string | null): string {
  if (!value) return "Not recorded";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Date unavailable";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date);
}

function ReadOnlyResearch({
  paper,
  onStartNew,
  onStartRevision,
}: {
  paper: ResearchPaper;
  onStartNew?: () => void;
  onStartRevision?: () => void;
}) {
  const paragraphs = paper.content.split(/\n\s*\n/).filter((paragraph) => paragraph.trim());
  return (
    <section aria-labelledby="research-submitted-heading">
      <h2 id="research-submitted-heading">Submitted research</h2>
      <p role="status" aria-live="polite">
        This paper is {statusLabel(paper.status)} and is now immutable. Submitted {formatDate(paper.submitted_at)}.
      </p>
      <article aria-labelledby="research-submitted-title">
        <h3 id="research-submitted-title">{paper.title}</h3>
        {paper.abstract && (
          <section aria-labelledby="research-submitted-abstract">
            <h4 id="research-submitted-abstract">Abstract</h4>
            <p>{paper.abstract}</p>
          </section>
        )}
        <section aria-labelledby="research-submitted-content">
          <h4 id="research-submitted-content">Paper content</h4>
          <div aria-label="Submitted paper content">
            {paragraphs.length > 0 ? paragraphs.map((paragraph, index) => (
              <p key={`submitted-paragraph-${index}`}>{paragraph}</p>
            )) : <p>{paper.content}</p>}
          </div>
        </section>
      </article>
      <p>Paper ID: <code>{paper.id}</code></p>
      {paper.status === "rejected" && onStartRevision && (
        <Button type="button" onClick={onStartRevision}>
          Create revision
        </Button>
      )}
      {paper.status !== "rejected" && onStartNew && (
        <Button type="button" onClick={onStartNew}>
          Start a new draft
        </Button>
      )}
    </section>
  );
}

export function ResearchEditor({
  paper,
  revisionSourceId,
  onSaved,
  onSubmitted,
  onCancel,
  onStartRevision,
}: ResearchEditorProps) {
  const createDraft = useCreateResearchDraft();
  const createRevision = useCreateResearchRevision();
  const updateDraft = useUpdateResearchDraft();
  const submitPaper = useSubmitResearchPaper();
  const [values, setValues] = useState<EditorValues>(() => valuesFromPaper(paper));
  const [savedPaper, setSavedPaper] = useState<ResearchPaper | undefined>(paper);
  const [retryAction, setRetryAction] = useState<RetryAction>("save");
  const [pendingSubmission, setPendingSubmission] = useState<ResearchPaper | null>(null);
  const [validationErrors, setValidationErrors] = useState<ValidationErrors>({});
  const confirmSubmissionRef = useRef<HTMLButtonElement>(null);
  const revisionIdRef = useRef<string | null>(null);

  useEffect(() => {
    setValues(valuesFromPaper(paper));
    setSavedPaper(paper);
    setValidationErrors({});
    revisionIdRef.current = null;
  }, [paper?.id, paper?.updated_at, revisionSourceId]);

  const currentPaper = savedPaper ?? paper;
  const creatingRevision = Boolean(revisionSourceId) && currentPaper?.status === "rejected";
  const editable = creatingRevision || !currentPaper || currentPaper.status === "draft";
  const busy = createDraft.isPending || createRevision.isPending || updateDraft.isPending || submitPaper.isPending;
  const requestError = createDraft.error ?? createRevision.error ?? updateDraft.error ?? submitPaper.error;

  function updateField(field: DraftField, value: string) {
    setValues((current) => ({ ...current, [field]: value }));
    setValidationErrors((current) => {
      if (!current[field]) return current;
      const next = { ...current };
      delete next[field];
      return next;
    });
  }

  async function persist(wantsSubmit: boolean) {
    setRetryAction(wantsSubmit ? "prepare-submit" : "save");
    const nextValidationErrors = validateResearchDraft(values);
    setValidationErrors(nextValidationErrors);
    if (Object.keys(nextValidationErrors).length > 0) return;

    const input = {
      title: normalizeText(values.title).trim(),
      abstract: normalizeText(values.abstract).trim(),
      content: normalizeText(values.content).trim(),
    };
    let nextPaper: ResearchPaper;
    if (creatingRevision && revisionSourceId) {
      const newPaperId = revisionIdRef.current ?? crypto.randomUUID();
      revisionIdRef.current = newPaperId;
      nextPaper = await createRevision.mutateAsync({
        sourceId: revisionSourceId,
        input: { ...input, new_paper_id: newPaperId },
      });
    } else if (currentPaper) {
      nextPaper = await updateDraft.mutateAsync({ id: currentPaper.id, input });
    } else {
      nextPaper = await createDraft.mutateAsync(input);
    }
    setSavedPaper(nextPaper);
    onSaved?.(nextPaper);
    if (wantsSubmit) {
      setPendingSubmission(nextPaper);
    }
  }

  async function confirmSubmission() {
    if (!pendingSubmission) return;
    setRetryAction("submit");
    const submitted = await submitPaper.mutateAsync(pendingSubmission.id);
    setSavedPaper(submitted);
    setPendingSubmission(null);
    onSubmitted?.(submitted);
  }

  function cancelSubmission() {
    setPendingSubmission(null);
    submitPaper.reset();
    setRetryAction("save");
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const submitter = (event.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null;
    void persist(submitter?.value === "submit").catch(() => undefined);
  }

  if (!editable) {
    return currentPaper ? (
      <ReadOnlyResearch paper={currentPaper} onStartNew={onCancel} onStartRevision={onStartRevision} />
    ) : null;
  }

  return (
    <section aria-labelledby="research-editor-heading">
      <h2 id="research-editor-heading">
        {creatingRevision ? "Create research revision" : currentPaper ? "Edit research draft" : "Create research draft"}
      </h2>
      {Object.keys(validationErrors).length > 0 && (
        <Alert>
          <p>Review the highlighted fields before saving.</p>
          <ul>
            {Object.entries(validationErrors).map(([field, message]) => (
              <li key={field}>{message}</li>
            ))}
          </ul>
        </Alert>
      )}
      {requestError && (
        <Alert>
          <p>{requestErrorMessage(requestError)}</p>
          <Button
            type="button"
            onClick={() => void (retryAction === "submit" ? confirmSubmission() : persist(retryAction === "prepare-submit")).catch(() => undefined)}
            disabled={busy}
          >
            Retry {retryAction === "submit" ? "submission" : "save"}
          </Button>
        </Alert>
      )}
      <Form
        onSubmit={handleSubmit}
        description="Plain text only. Your draft can be saved and recovered before you submit it for review."
        descriptionId="research-editor-help"
        aria-hidden={Boolean(pendingSubmission)}
      >
        <fieldset disabled={busy || Boolean(pendingSubmission)}>
          <legend><VisuallyHidden>Research draft fields</VisuallyHidden></legend>
          <TextField
            id="research-title"
            name="title"
            label="Title"
            value={values.title}
            onChange={(event) => updateField("title", event.target.value)}
            maxLength={200}
            required
            error={validationErrors.title}
            count={`${characterCount(values.title)}/200 characters`}
          />
          <TextField
            id="research-abstract"
            name="abstract"
            label="Abstract"
            value={values.abstract}
            onChange={(event) => updateField("abstract", event.target.value)}
            maxLength={5_000}
            rows={5}
            multiline
            error={validationErrors.abstract}
            count={`${characterCount(values.abstract)}/5000 characters`}
          />
          <MarkdownEditor
            id="research-content"
            name="content"
            label="Paper content"
            value={values.content}
            onChange={(event) => updateField("content", event.target.value)}
            maxLength={500_000}
            required
            description="Plain text only; markup and unsafe links are not accepted."
            error={validationErrors.content}
            count={`${characterCount(values.content)}/500000 characters`}
          />
        </fieldset>
        <LiveRegion>
          {busy ? (retryAction === "submit" ? "Submitting your research…" : "Saving your research…") : null}
        </LiveRegion>
        <Button type="submit" value="save" disabled={busy || Boolean(pendingSubmission)}>
          {busy && retryAction === "save" ? "Saving…" : "Save draft"}
        </Button>{" "}
        <Button type="submit" value="submit" disabled={busy || Boolean(pendingSubmission)}>
          {busy && retryAction === "prepare-submit" ? "Saving…" : "Save and submit for review"}
        </Button>{" "}
        {onCancel && (
          <Button type="button" onClick={onCancel} disabled={busy || Boolean(pendingSubmission)}>
            Start a new draft
          </Button>
        )}
      </Form>
      <Modal
        open={Boolean(pendingSubmission)}
        title="Submit this paper for review?"
        description={pendingSubmission ? (
          <>After you confirm, <strong>{pendingSubmission.title}</strong> will be submitted for review and its content will be immutable.
            You will be able to track its status, but you cannot edit this version.</>
        ) : undefined}
        onClose={busy ? undefined : cancelSubmission}
        initialFocusRef={confirmSubmissionRef}
      >
          {requestError && retryAction === "submit" && (
            <Alert>Submission failed. You can retry without changing the saved draft.</Alert>
          )}
          <Button ref={confirmSubmissionRef} type="button" variant="primary" onClick={() => void confirmSubmission().catch(() => undefined)} disabled={busy}>
            {busy && retryAction === "submit" ? "Submitting…" : "Confirm and submit for review"}
          </Button>{" "}
          <Button type="button" onClick={cancelSubmission} disabled={busy}>
            Keep editing
          </Button>
      </Modal>
      {currentPaper && (
        <p>
          Draft ID: <code>{currentPaper.id}</code>
        </p>
      )}
    </section>
  );
}
