export type NewsArticle = Readonly<{
  id: string;
  source_id: string;
  source_name: string;
  source_slug: string;
  title: string;
  summary: string;
  content: string;
  url: string;
  image_url: string | null;
  author: string | null;
  category: string | null;
  symbols: readonly string[];
  published_at: string;
}>;

export type NewsFeedResponse = Readonly<{
  items: readonly NewsArticle[];
  limit: number;
  offset: number;
  has_more: boolean;
  next_cursor: string | null;
}>;

export type NewsFeedQuery = Readonly<{
  limit?: number;
  cursor?: string;
  category?: string;
  symbol?: string;
  source_id?: string;
}>;

/**
 * Returns a canonical URL only for explicit HTTP(S) destinations. React
 * escapes the article text separately; this function protects the href.
 */
export function safeOutboundUrl(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;

  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    return null;
  }

  if ((url.protocol !== "http:" && url.protocol !== "https:") ||
      !url.hostname || url.username || url.password) {
    return null;
  }

  return url.toString();
}

export function formatNewsPublishedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Time unavailable";

  return `${new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date)} UTC`;
}

export function normalizedNewsPublishedAt(value: string): string | undefined {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString();
}
