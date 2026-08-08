\set ON_ERROR_STOP on
\timing on

-- Synthetic research workload: 10,000 authors, 250,000 papers, and two
-- reviews per paper.  The temporary tables mirror the production columns used
-- by the research queries, allowing this script to run independently of the
-- incomplete scaffold migrations.
CREATE TEMP TABLE users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL
);

CREATE TEMP TABLE research_papers (
    id UUID PRIMARY KEY,
    author_id UUID NOT NULL,
    title TEXT NOT NULL,
    abstract TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL,
    submitted_at TIMESTAMPTZ,
    under_review_at TIMESTAMPTZ,
    decided_by UUID,
    decided_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    evaluation_score DOUBLE PRECISION,
    evaluation_result JSONB,
    elo_award INTEGER,
    elo_awarded BOOLEAN NOT NULL DEFAULT FALSE,
    elo_awarded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TEMP TABLE research_reviews (
    id UUID PRIMARY KEY,
    paper_id UUID NOT NULL,
    reviewer_id UUID NOT NULL,
    score DOUBLE PRECISION,
    recommendation TEXT NOT NULL,
    comments TEXT,
    evaluation_result JSONB,
    reviewed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

INSERT INTO users (id, username)
SELECT
    md5(user_number::text)::uuid,
    'synthetic_user_' || user_number
FROM generate_series(1, 10000) AS source(user_number);

INSERT INTO research_papers (
    id,
    author_id,
    title,
    abstract,
    content,
    status,
    submitted_at,
    under_review_at,
    decided_by,
    decided_at,
    published_at,
    evaluation_score,
    created_at,
    updated_at
)
SELECT
    md5('paper:' || paper_number::text)::uuid,
    md5(('author:' || (((paper_number - 1) % 10000) + 1)::text))::uuid,
    'Synthetic research paper ' || paper_number,
    'Synthetic abstract ' || paper_number,
    repeat('synthetic research content ', 8),
    CASE paper_number % 6
        WHEN 0 THEN 'draft'
        WHEN 1 THEN 'submitted'
        WHEN 2 THEN 'under_review'
        WHEN 3 THEN 'approved'
        WHEN 4 THEN 'rejected'
        ELSE 'published'
    END,
    CASE WHEN paper_number % 6 = 0 THEN NULL ELSE CURRENT_TIMESTAMP - (paper_number % 365) * INTERVAL '1 day' END,
    CASE WHEN paper_number % 6 IN (2, 3, 4, 5) THEN CURRENT_TIMESTAMP - (paper_number % 300) * INTERVAL '1 day' END,
    CASE WHEN paper_number % 6 IN (3, 4, 5) THEN md5(('author:' || (((paper_number + 97) % 10000) + 1)::text))::uuid END,
    CASE WHEN paper_number % 6 IN (3, 4, 5) THEN CURRENT_TIMESTAMP - (paper_number % 250) * INTERVAL '1 day' END,
    CASE WHEN paper_number % 6 = 5 THEN CURRENT_TIMESTAMP - (paper_number % 200) * INTERVAL '1 day' END,
    CASE WHEN paper_number % 6 IN (3, 4, 5) THEN (paper_number % 101)::double precision END,
    CURRENT_TIMESTAMP - (paper_number % 730) * INTERVAL '1 day',
    CURRENT_TIMESTAMP
FROM generate_series(1, 250000) AS source(paper_number);

INSERT INTO research_reviews (
    id,
    paper_id,
    reviewer_id,
    score,
    recommendation,
    comments,
    evaluation_result,
    reviewed_at,
    created_at,
    updated_at
)
SELECT
    md5('review:' || review_number::text)::uuid,
    md5('paper:' || paper_number::text)::uuid,
    md5(('author:' || (((paper_number + reviewer_offset - 1) % 10000) + 1)::text))::uuid,
    ((review_number * 17) % 101)::double precision,
    CASE WHEN review_number % 2 = 0 THEN 'approve' ELSE 'reject' END,
    'Synthetic review ' || review_number,
    jsonb_build_object('source', 'synthetic', 'dimension', review_number % 8),
    CURRENT_TIMESTAMP - (review_number % 365) * INTERVAL '1 day',
    CURRENT_TIMESTAMP - (review_number % 365) * INTERVAL '1 day',
    CURRENT_TIMESTAMP
FROM generate_series(1, 500000) AS source(review_number)
CROSS JOIN LATERAL (
    SELECT ((review_number - 1) / 2) + 1 AS paper_number,
           CASE WHEN review_number % 2 = 0 THEN 5001 ELSE 1 END AS reviewer_offset
) AS generated;

CREATE INDEX research_papers_author_created_idx
    ON research_papers (author_id, created_at DESC, id DESC);

CREATE INDEX research_papers_review_queue_idx
    ON research_papers (submitted_at ASC, id ASC)
    WHERE status IN ('submitted', 'under_review');

CREATE INDEX research_papers_published_idx
    ON research_papers (published_at DESC, id DESC)
    WHERE status = 'published';

CREATE INDEX research_reviews_paper_created_idx
    ON research_reviews (paper_id, created_at ASC, id ASC);

CREATE UNIQUE INDEX research_reviews_paper_reviewer_uidx
    ON research_reviews (paper_id, reviewer_id);

ANALYZE users;
ANALYZE research_papers;
ANALYZE research_reviews;

DO $$
BEGIN
    IF (SELECT COUNT(*) FROM users) <> 10000 THEN
        RAISE EXCEPTION 'synthetic user dataset is incomplete';
    END IF;

    IF (SELECT COUNT(*) FROM research_papers) <> 250000 THEN
        RAISE EXCEPTION 'synthetic paper dataset is incomplete';
    END IF;

    IF (SELECT COUNT(*) FROM research_reviews) <> 500000 THEN
        RAISE EXCEPTION 'synthetic review dataset is incomplete';
    END IF;

    IF EXISTS (
        SELECT paper_id, reviewer_id
        FROM research_reviews
        GROUP BY paper_id, reviewer_id
        HAVING COUNT(*) > 1
    ) THEN
        RAISE EXCEPTION 'synthetic reviews contain duplicate reviewer votes';
    END IF;
END
$$;

-- Author-owned paper listing.
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, author_id, title, status, created_at
FROM research_papers
WHERE author_id = md5('author:17')::uuid
ORDER BY created_at DESC, id DESC
LIMIT 50 OFFSET 0;

-- Pending review queue.
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, author_id, title, status, submitted_at
FROM research_papers
WHERE status IN ('submitted', 'under_review')
ORDER BY submitted_at ASC NULLS LAST, id ASC
LIMIT 100 OFFSET 0;

-- Published research feed.
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, author_id, title, published_at
FROM research_papers
WHERE status = 'published'
ORDER BY published_at DESC, id DESC
LIMIT 100 OFFSET 0;

-- Review history for one paper.
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, paper_id, reviewer_id, score, recommendation, reviewed_at
FROM research_reviews
WHERE paper_id = md5('paper:125000')::uuid
ORDER BY created_at ASC, id ASC;

-- Published-paper point lookup.
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, author_id, title, published_at
FROM research_papers
WHERE id = md5('paper:249995')::uuid
  AND status = 'published';

SELECT
    (SELECT COUNT(*) FROM research_papers WHERE status IN ('submitted', 'under_review')) AS pending_count,
    (SELECT COUNT(*) FROM research_papers WHERE status = 'published') AS published_count,
    (SELECT COUNT(*) FROM research_reviews) AS review_count;
