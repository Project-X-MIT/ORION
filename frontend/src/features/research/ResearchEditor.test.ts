import { describe, expect, it } from "vitest";

import { validateResearchDraft } from "./ResearchEditor";
import { lifecycleStatus } from "./ResearchPage";
import type { ResearchDraftInput, ResearchPaper } from "./types";

const validDraft: ResearchDraftInput = {
  title: "A research paper",
  abstract: "A concise summary.",
  content: "The paper content.\n\nA second paragraph.",
};

const serverPaper: ResearchPaper = {
  id: "paper-1",
  author_id: "author-1",
  title: validDraft.title,
  abstract: validDraft.abstract,
  content: validDraft.content,
  status: "published",
  submitted_at: "2026-08-16T08:00:00Z",
  under_review_at: "2026-08-16T09:00:00Z",
  decided_at: "2026-08-16T10:00:00Z",
  published_at: "2026-08-16T11:00:00Z",
  elo_award: null,
  elo_awarded: false,
  elo_awarded_at: null,
  created_at: "2026-08-16T07:00:00Z",
  updated_at: "2026-08-16T11:00:00Z",
};

describe("server-owned research lifecycle", () => {
  it("renders publication only when the API reports published", () => {
    expect(lifecycleStatus({ ...serverPaper, status: "approved" })).toBe("approved");
    expect(lifecycleStatus(serverPaper)).toBe("published");
  });

  it("renders an award only when the API reports an awarded paper", () => {
    expect(lifecycleStatus(serverPaper)).toBe("published");
    expect(lifecycleStatus({
      ...serverPaper,
      status: "approved",
      elo_award: 25,
      elo_awarded: true,
      elo_awarded_at: "2026-08-16T12:00:00Z",
    })).toBe("approved");
    expect(lifecycleStatus({
      ...serverPaper,
      elo_award: 25,
      elo_awarded: true,
      elo_awarded_at: "2026-08-16T12:00:00Z",
    })).toBe("awarded");
  });
});

describe("validateResearchDraft", () => {
  it("accepts a valid plain-text draft", () => {
    expect(validateResearchDraft(validDraft)).toEqual({});
  });

  it("requires title and content after trimming whitespace", () => {
    expect(validateResearchDraft({ ...validDraft, title: "  ", content: "\n\t" })).toMatchObject({
      title: "Title is required.",
      content: "Paper content is required.",
    });
  });

  it("enforces the field length policies", () => {
    expect(validateResearchDraft({ ...validDraft, title: "x".repeat(201) }).title)
      .toBe("Title must be 200 characters or fewer.");
    expect(validateResearchDraft({ ...validDraft, abstract: "x".repeat(5_001) }).abstract)
      .toBe("Abstract must be 5,000 characters or fewer.");
  });

  it("rejects markup, unsafe schemes, and control characters", () => {
    expect(validateResearchDraft({ ...validDraft, content: "<script>alert(1)</script>" }).content)
      .toContain("plain text only");
    expect(validateResearchDraft({ ...validDraft, abstract: "javascript:alert(1)" }).abstract)
      .toContain("plain text only");
    expect(validateResearchDraft({ ...validDraft, content: `safe${String.fromCharCode(0)}text` }).content)
      .toContain("plain text only");
  });
});
