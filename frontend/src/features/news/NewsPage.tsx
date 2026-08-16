import { useNewsFeed } from "./hooks";
import "./NewsPage.css";
import {
  formatNewsPublishedAt,
  normalizedNewsPublishedAt,
  safeOutboundUrl,
  type NewsArticle,
} from "./types";

function articleSource(article: NewsArticle): string {
  return article.source_name.trim() || article.source_slug.trim() || "Unknown source";
}

function articleTitle(article: NewsArticle): string {
  return article.title.trim() || "Untitled article";
}

function articleSummary(article: NewsArticle): string {
  return article.summary.trim() || "No summary available.";
}

export function NewsArticleCard({ article }: { article: NewsArticle }) {
  const href = safeOutboundUrl(article.url);
  const publishedAt = normalizedNewsPublishedAt(article.published_at);

  return (
    <article aria-labelledby={`news-title-${article.id}`} className="news-article">
      <header className="news-article__header">
        <p className="news-article__source">{articleSource(article)}</p>
        <time
          className="news-article__time"
          dateTime={publishedAt}
          title={publishedAt ? new Date(publishedAt).toISOString() : undefined}
        >
          {formatNewsPublishedAt(article.published_at)}
        </time>
      </header>
      <h2 id={`news-title-${article.id}`}>{articleTitle(article)}</h2>
      <p className="news-article__summary">{articleSummary(article)}</p>
      {href ? (
        <a
          aria-label={`Read ${articleTitle(article)} from ${articleSource(article)} (opens in a new tab)`}
          href={href}
          referrerPolicy="no-referrer"
          rel="noopener noreferrer"
          target="_blank"
        >
          Read original article
        </a>
      ) : (
        <p role="status">Original article link unavailable.</p>
      )}
    </article>
  );
}

export function NewsPage() {
  const feed = useNewsFeed();

  if (feed.isPending && !feed.data) {
    return (
      <main aria-busy="true" aria-live="polite" className="news-feed">
        <h1>Market news</h1>
        <p>Loading news...</p>
      </main>
    );
  }

  if (feed.isError && !feed.data) {
    return (
      <main className="news-feed" role="alert">
        <h1>Market news</h1>
        <p role="status">No recent articles are available right now.</p>
        <p>News is temporarily unavailable. Please try again.</p>
        <button type="button" onClick={() => void feed.refetch()}>Try again</button>
      </main>
    );
  }

  const isStale = feed.isStale || feed.isRefetchError;
  const articles = feed.data.items;
  return (
    <main className="news-feed">
      <header className="news-feed__header">
        <h1>Market news</h1>
        {feed.isFetching && !isStale ? <p role="status">Refreshing news...</p> : null}
      </header>
      {isStale ? (
        <aside
          aria-live="polite"
          className="news-feed__stale"
          role={feed.isRefetchError ? "alert" : "status"}
        >
          <strong>News may be stale</strong>
          <p>
            {feed.isRefetchError
              ? "We could not refresh the latest articles."
              : "This feed may be out of date."}
          </p>
          <button disabled={feed.isFetching} type="button" onClick={() => void feed.refetch()}>
            {feed.isFetching ? "Refreshing..." : "Refresh news"}
          </button>
        </aside>
      ) : null}
      {feed.isError && !feed.isRefetchError ? (
        <p role="alert">The news feed could not be refreshed. Showing the last available articles.</p>
      ) : null}
      {articles.length === 0 ? (
        <p role="status">No news is available right now.</p>
      ) : (
        <section aria-label="Latest market news" className="news-feed__list">
          {articles.map((article) => <NewsArticleCard article={article} key={article.id} />)}
        </section>
      )}
    </main>
  );
}
