import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { NewsArticleCard, NewsPage } from "./NewsPage";
import { useNewsFeed } from "./hooks";
import { safeOutboundUrl, type NewsArticle } from "./types";

vi.mock("./hooks", () => ({ useNewsFeed: vi.fn() }));

const mockedUseNewsFeed = vi.mocked(useNewsFeed);

const article: NewsArticle = {
  id: "article-1",
  source_id: "source-1",
  source_name: "SEC",
  source_slug: "sec",
  title: "Markets open higher",
  summary: "A concise market summary.",
  content: "Full article content is not rendered in this card.",
  url: "https://www.sec.gov/news/story",
  image_url: null,
  author: null,
  category: "markets",
  symbols: ["ORION"],
  published_at: "2026-08-14T10:00:00Z",
};

describe("NewsArticleCard", () => {
  it("renders source, UTC time, summary, and a protected outbound link", () => {
    const markup = renderToStaticMarkup(<NewsArticleCard article={article} />);

    expect(markup).toContain("SEC");
    expect(markup).toContain('dateTime="2026-08-14T10:00:00.000Z"');
    expect(markup).toContain("UTC");
    expect(markup).toContain("A concise market summary.");
    expect(markup).toContain('href="https://www.sec.gov/news/story"');
    expect(markup).toContain('target="_blank"');
    expect(markup).toContain('rel="noopener noreferrer"');
  });

  it("escapes article text and omits unsafe outbound URLs", () => {
    const markup = renderToStaticMarkup(
      <NewsArticleCard
        article={{
          ...article,
          summary: "<script>alert(1)</script>",
          url: "javascript:alert(1)",
        }}
      />,
    );

    expect(markup).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(markup).not.toContain('href="javascript:alert(1)"');
    expect(markup).toContain("Original article link unavailable.");
  });
});

describe("safeOutboundUrl", () => {
  it.each(["javascript:alert(1)", "data:text/html,unsafe", "//example.com/story", ""]) (
    "rejects %s",
    (value) => {
      expect(safeOutboundUrl(value)).toBeNull();
    },
  );

  it("allows explicit HTTP(S) URLs without credentials", () => {
    expect(safeOutboundUrl(" https://example.com/story "))
      .toBe("https://example.com/story");
    expect(safeOutboundUrl("http://example.com/story"))
      .toBe("http://example.com/story");
    expect(safeOutboundUrl("https://user:password@example.com/story"))
      .toBeNull();
  });
});

describe("NewsPage states", () => {
  beforeEach(() => {
    mockedUseNewsFeed.mockReset();
  });

  it("renders an accessible loading state", () => {
    mockedUseNewsFeed.mockReturnValue({
      data: undefined,
      isPending: true,
      isError: false,
      refetch: vi.fn(),
    } as ReturnType<typeof useNewsFeed>);

    const markup = renderToStaticMarkup(<NewsPage />);
    expect(markup).toContain('aria-busy="true"');
    expect(markup).toContain("Loading news...");
  });

  it("renders the empty state", () => {
    mockedUseNewsFeed.mockReturnValue({
      data: { items: [], limit: 20, offset: 0, has_more: false, next_cursor: null },
      isPending: false,
      isError: false,
      isStale: false,
      isRefetchError: false,
      isFetching: false,
      refetch: vi.fn(),
    } as ReturnType<typeof useNewsFeed>);

    expect(renderToStaticMarkup(<NewsPage />)).toContain("No news is available right now.");
  });

  it("renders a recoverable error when no feed data exists", () => {
    mockedUseNewsFeed.mockReturnValue({
      data: undefined,
      isPending: false,
      isError: true,
      refetch: vi.fn(),
    } as ReturnType<typeof useNewsFeed>);

    const markup = renderToStaticMarkup(<NewsPage />);
    expect(markup).toContain("News is temporarily unavailable.");
    expect(markup).toContain("No recent articles are available right now.");
    expect(markup).toContain("Try again");
  });

  it("marks cached data as stale and offers recovery", () => {
    mockedUseNewsFeed.mockReturnValue({
      data: { items: [article], limit: 20, offset: 0, has_more: false, next_cursor: null },
      isPending: false,
      isError: false,
      isStale: true,
      isRefetchError: false,
      isFetching: false,
      refetch: vi.fn(),
    } as ReturnType<typeof useNewsFeed>);

    const markup = renderToStaticMarkup(<NewsPage />);
    expect(markup).toContain("News may be stale");
    expect(markup).toContain("This feed may be out of date.");
    expect(markup).toContain("Refresh news");
    expect(markup).toContain("Markets open higher");
  });

  it("keeps cached articles visible after a refresh error", () => {
    mockedUseNewsFeed.mockReturnValue({
      data: { items: [article], limit: 20, offset: 0, has_more: false, next_cursor: null },
      isPending: false,
      isError: true,
      isStale: true,
      isRefetchError: true,
      isFetching: false,
      refetch: vi.fn(),
    } as ReturnType<typeof useNewsFeed>);

    const markup = renderToStaticMarkup(<NewsPage />);
    expect(markup).toContain("We could not refresh the latest articles.");
    expect(markup).toContain("Markets open higher");
  });
});
