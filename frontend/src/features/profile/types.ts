export interface ProfileRatingPoint {
  occurred_at: string;
  quiz_type: string;
  rating_before: number;
  rating_after: number;
  rating_delta: number;
  correct: boolean;
}

export interface ProfileRankPoint {
  snapshot_at: string;
  previous_rank: number | null;
  current_rank: number;
  rank_movement: number | null;
}

export interface ProfilePerformancePoint {
  completed_at: string;
  quiz_type: string;
  total_questions: number;
  correct_answers: number;
  score: number;
  rating_after: number;
}

export interface PublishedResearch {
  id: string;
  title: string;
  abstract: string;
  published_at: string;
  evaluation_score: number | null;
  evaluated_content_version: number | null;
  elo_award: number | null;
  elo_awarded: boolean;
}

export interface Profile {
  schema_version: number;
  user_id: string;
  username: string;
  display_name: string | null;
  bio: string | null;
  avatar_url: string | null;
  rating: number | null;
  global_rank: number | null;
  rank_movement: number | null;
  quizzes_completed: number;
  correct_answers: number;
  rating_history: ProfileRatingPoint[];
  rank_history: ProfileRankPoint[];
  performance_history: ProfilePerformancePoint[];
  published_research: PublishedResearch[];
}

export function formatProfileDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "Unknown date" : date.toLocaleDateString();
}
