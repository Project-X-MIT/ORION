import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { useAuth } from "../../providers/AuthProvider";
import { useProfile } from "./hooks";
import { ProfilePage } from "./ProfilePage";
import type { Profile } from "./types";

vi.mock("../../providers/AuthProvider", () => ({ useAuth: vi.fn() }));
vi.mock("./hooks", () => ({ useProfile: vi.fn() }));

const mockedAuth = vi.mocked(useAuth);
const mockedProfile = vi.mocked(useProfile);

const profile: Profile = {
  schema_version: 1,
  user_id: "00000000-0000-0000-0000-000000000001",
  username: "orion",
  display_name: "Orion",
  bio: "Public research profile",
  avatar_url: null,
  rating: 1510,
  global_rank: 4,
  rank_movement: 2,
  quizzes_completed: 3,
  correct_answers: 8,
  rating_history: [{
    occurred_at: "2026-08-17T10:00:00Z",
    quiz_type: "basic",
    rating_before: 1500,
    rating_after: 1510,
    rating_delta: 10,
    correct: true,
  }],
  rank_history: [{
    snapshot_at: "2026-08-17T10:00:00Z",
    previous_rank: 6,
    current_rank: 4,
    rank_movement: 2,
  }],
  performance_history: [{
    completed_at: "2026-08-17T10:00:00Z",
    quiz_type: "basic",
    total_questions: 10,
    correct_answers: 8,
    score: 80,
    rating_after: 1510,
  }],
  published_research: [{
    id: "paper-1",
    title: "Published report",
    abstract: "A public abstract",
    published_at: "2026-08-16T10:00:00Z",
    evaluation_score: 91,
    evaluated_content_version: 1,
    elo_award: 25,
    elo_awarded: true,
  }],
};

describe("ProfilePage", () => {
  beforeEach(() => {
    mockedAuth.mockReturnValue({ user: { id: profile.user_id, username: profile.username, display_name: profile.display_name } } as unknown as ReturnType<typeof useAuth>);
    mockedProfile.mockReturnValue({ data: profile, isPending: false, isError: false, isFetching: false, refetch: vi.fn() } as unknown as ReturnType<typeof useProfile>);
  });

  it("renders authoritative identity, rating, movement, and awarded research", () => {
    const markup = renderToStaticMarkup(<ProfilePage userId={profile.user_id} />);
    expect(markup).toContain("Orion");
    expect(markup).toContain("#4");
    expect(markup).toContain("↑ 2");
    expect(markup).toContain("Published report");
    expect(markup).toContain("Awarded 25 Elo");
  });

  it("keeps every visual chart paired with an accessible table", () => {
    const markup = renderToStaticMarkup(<ProfilePage userId={profile.user_id} />);
    expect(markup).toContain('aria-label="Elo rating history chart"');
    expect(markup).toContain('aria-label="Global rank history chart; lower rank numbers are better"');
    expect(markup).toContain('aria-label="Quiz score history chart"');
    expect(markup).toContain("Elo history data");
    expect(markup).toContain("Rank history data");
    expect(markup).toContain("Quiz performance data");
  });

  it("renders a recoverable loading and error path", () => {
    mockedProfile.mockReturnValue({ data: undefined, isPending: true, isError: false, refetch: vi.fn() } as unknown as ReturnType<typeof useProfile>);
    expect(renderToStaticMarkup(<ProfilePage userId={profile.user_id} />)).toContain('aria-busy="true"');
    mockedProfile.mockReturnValue({ data: undefined, isPending: false, isError: true, refetch: vi.fn() } as unknown as ReturnType<typeof useProfile>);
    expect(renderToStaticMarkup(<ProfilePage userId={profile.user_id} />)).toContain("Profile unavailable");
  });
});
