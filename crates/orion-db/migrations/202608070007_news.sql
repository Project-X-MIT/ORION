-- News sources are kept separate from articles so a feed can be configured
-- once and its ingestion history can be audited independently.
CREATE TABLE news_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    external_id TEXT UNIQUE,
    source_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT news_sources_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT news_sources_slug_not_blank CHECK (btrim(slug) <> ''),
    CONSTRAINT news_sources_external_id_not_blank
        CHECK (external_id IS NULL OR btrim(external_id) <> ''),
    CONSTRAINT news_sources_url_not_blank
        CHECK (source_url IS NULL OR btrim(source_url) <> '')
);

CREATE TABLE news_articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES news_sources (id) ON DELETE RESTRICT,
    external_id TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    content TEXT NOT NULL,
    url TEXT NOT NULL UNIQUE,
    image_url TEXT,
    author TEXT,
    category TEXT,
    symbols TEXT[] NOT NULL DEFAULT '{}',
    published_at TIMESTAMPTZ NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT news_articles_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT news_articles_summary_not_blank CHECK (btrim(summary) <> ''),
    CONSTRAINT news_articles_content_not_blank CHECK (btrim(content) <> ''),
    CONSTRAINT news_articles_url_not_blank CHECK (btrim(url) <> ''),
    CONSTRAINT news_articles_external_id_not_blank
        CHECK (external_id IS NULL OR btrim(external_id) <> '')
);

-- Some providers do not expose a stable external ID. The partial unique index
-- still deduplicates IDs when they are available without treating NULL as a
-- single shared value.
CREATE UNIQUE INDEX news_articles_source_external_id_idx
    ON news_articles (source_id, external_id)
    WHERE external_id IS NOT NULL;

CREATE TABLE news_ingestion_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_id UUID NOT NULL REFERENCES news_sources (id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'running',
    articles_seen INTEGER NOT NULL DEFAULT 0 CHECK (articles_seen >= 0),
    articles_inserted INTEGER NOT NULL DEFAULT 0 CHECK (articles_inserted >= 0),
    error_message TEXT,

    CONSTRAINT news_ingestion_runs_status_check
        CHECK (status IN ('running', 'completed', 'failed')),
    CONSTRAINT news_ingestion_runs_completion_check
        CHECK (status = 'running' OR completed_at IS NOT NULL)
);

CREATE INDEX news_articles_published_at_idx
    ON news_articles (published_at DESC, id DESC);

CREATE INDEX news_articles_source_published_at_idx
    ON news_articles (source_id, published_at DESC, id DESC);

CREATE INDEX news_articles_category_published_at_idx
    ON news_articles (category, published_at DESC, id DESC)
    WHERE category IS NOT NULL;

CREATE INDEX news_articles_symbols_gin_idx
    ON news_articles USING GIN (symbols);

CREATE INDEX news_ingestion_runs_source_started_at_idx
    ON news_ingestion_runs (source_id, started_at DESC);
