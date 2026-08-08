CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE research_papers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (length(btrim(title)) > 0),
    abstract TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL CHECK (length(btrim(content)) > 0),
    status TEXT NOT NULL DEFAULT 'draft' CHECK (
        status IN ('draft', 'submitted', 'under_review', 'approved', 'rejected', 'published')
    ),
    submitted_at TIMESTAMPTZ,
    under_review_at TIMESTAMPTZ,
    decided_by UUID REFERENCES users (id) ON DELETE RESTRICT,
    decided_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    evaluation_score DOUBLE PRECISION CHECK (evaluation_score BETWEEN 0 AND 100),
    evaluation_result JSONB,
    elo_award INTEGER CHECK (elo_award >= 0),
    elo_awarded BOOLEAN NOT NULL DEFAULT FALSE,
    elo_awarded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT research_papers_elo_award_state_ck CHECK (
        (NOT elo_awarded AND elo_award IS NULL AND elo_awarded_at IS NULL)
        OR (elo_awarded AND elo_award IS NOT NULL AND elo_awarded_at IS NOT NULL)
    ),

    CONSTRAINT research_papers_lifecycle_timestamps_ck CHECK (
        (status NOT IN ('submitted', 'under_review', 'approved', 'rejected', 'published')
            OR submitted_at IS NOT NULL)
        AND (status NOT IN ('under_review', 'approved', 'rejected', 'published')
            OR under_review_at IS NOT NULL)
        AND (status NOT IN ('approved', 'rejected', 'published')
            OR (decided_by IS NOT NULL AND decided_at IS NOT NULL))
        AND (status <> 'published' OR published_at IS NOT NULL)
    )
);

CREATE TABLE research_reviews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    paper_id UUID NOT NULL REFERENCES research_papers (id) ON DELETE CASCADE,
    reviewer_id UUID NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    score DOUBLE PRECISION CHECK (score BETWEEN 0 AND 100),
    recommendation TEXT NOT NULL CHECK (
        recommendation IN ('approve', 'approved', 'reject', 'rejected')
    ),
    comments TEXT,
    evaluation_result JSONB,
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- A reviewer may retry its own evaluation, but may not create duplicate votes
-- for the same paper.
CREATE UNIQUE INDEX research_reviews_paper_reviewer_uidx
    ON research_reviews (paper_id, reviewer_id);

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

CREATE OR REPLACE FUNCTION research_papers_set_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION research_reviews_set_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION research_reviews_guard_author()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM research_papers
        WHERE id = NEW.paper_id
          AND author_id = NEW.reviewer_id
    ) THEN
        RAISE EXCEPTION 'a paper author cannot review their own paper'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION research_reviews_guard_decided_paper()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM research_papers
            WHERE id = OLD.paper_id
              AND status IN ('approved', 'rejected', 'published')
        ) THEN
            RAISE EXCEPTION 'reviews for decided research cannot be deleted'
                USING ERRCODE = 'check_violation';
        END IF;
        RETURN OLD;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM research_papers
        WHERE id = OLD.paper_id
          AND status IN ('approved', 'rejected', 'published')
    ) AND (
        NEW.paper_id IS DISTINCT FROM OLD.paper_id
        OR NEW.reviewer_id IS DISTINCT FROM OLD.reviewer_id
        OR NEW.recommendation IS DISTINCT FROM OLD.recommendation
    ) THEN
        RAISE EXCEPTION 'review identity and recommendation are immutable after decision'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION research_papers_guard_status_transition()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' AND NEW.status <> 'draft' THEN
        RAISE EXCEPTION 'research papers must be created in draft state'
            USING ERRCODE = 'check_violation';
    END IF;

    IF TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;

    IF OLD.status IN ('approved', 'rejected', 'published') AND (
        NEW.decided_by IS DISTINCT FROM OLD.decided_by
        OR NEW.decided_at IS DISTINCT FROM OLD.decided_at
    ) THEN
        RAISE EXCEPTION 'research decision audit fields are immutable'
            USING ERRCODE = 'check_violation';
    END IF;

    IF NEW.status = OLD.status THEN
        IF NEW.status IN ('approved', 'rejected', 'published')
            AND (NEW.decided_by IS NULL OR NEW.decided_at IS NULL) THEN
            RAISE EXCEPTION 'decided research requires reviewer and decision timestamp'
                USING ERRCODE = 'check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF NOT (
        (OLD.status = 'draft' AND NEW.status = 'submitted')
        OR (OLD.status = 'submitted' AND NEW.status = 'under_review')
        OR (OLD.status = 'under_review' AND NEW.status IN ('approved', 'rejected'))
        OR (OLD.status = 'approved' AND NEW.status = 'published')
    ) THEN
        RAISE EXCEPTION 'invalid research paper status transition: % -> %', OLD.status, NEW.status
            USING ERRCODE = 'check_violation';
    END IF;

    IF NEW.status IN ('approved', 'rejected') AND NEW.decided_by IS NULL THEN
        RAISE EXCEPTION 'research approval or rejection requires a reviewer'
            USING ERRCODE = 'check_violation';
    ELSIF NEW.status = 'submitted' AND NEW.submitted_at IS NULL THEN
        NEW.submitted_at = CURRENT_TIMESTAMP;
    ELSIF NEW.status = 'under_review' AND NEW.under_review_at IS NULL THEN
        NEW.under_review_at = CURRENT_TIMESTAMP;
    ELSIF NEW.status IN ('approved', 'rejected') AND NEW.decided_at IS NULL THEN
        NEW.decided_at = CURRENT_TIMESTAMP;
    ELSIF NEW.status = 'published' AND NEW.published_at IS NULL THEN
        NEW.published_at = CURRENT_TIMESTAMP;
    END IF;

    IF NEW.status IN ('approved', 'rejected') AND NOT EXISTS (
        SELECT 1
        FROM research_reviews
        WHERE paper_id = NEW.id
          AND reviewer_id = NEW.decided_by
          AND (
              (NEW.status = 'approved' AND recommendation IN ('approve', 'approved'))
              OR (NEW.status = 'rejected' AND recommendation IN ('reject', 'rejected'))
          )
    ) THEN
        RAISE EXCEPTION 'research decision requires a matching persisted review'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION research_papers_guard_elo_award()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.elo_awarded AND (
        NOT NEW.elo_awarded
        OR NEW.elo_award IS DISTINCT FROM OLD.elo_award
        OR NEW.elo_awarded_at IS DISTINCT FROM OLD.elo_awarded_at
    ) THEN
        RAISE EXCEPTION 'research Elo award has already been processed'
            USING ERRCODE = 'check_violation';
    END IF;

    IF NEW.elo_awarded AND NEW.status <> 'published' THEN
        RAISE EXCEPTION 'research Elo awards require a published paper'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER research_papers_updated_at_trg
BEFORE UPDATE ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_set_updated_at();

CREATE TRIGGER research_reviews_updated_at_trg
BEFORE UPDATE ON research_reviews
FOR EACH ROW
EXECUTE FUNCTION research_reviews_set_updated_at();

CREATE TRIGGER research_reviews_author_guard_trg
BEFORE INSERT OR UPDATE OF paper_id, reviewer_id ON research_reviews
FOR EACH ROW
EXECUTE FUNCTION research_reviews_guard_author();

CREATE TRIGGER research_reviews_decision_audit_trg
BEFORE DELETE OR UPDATE OF paper_id, reviewer_id, recommendation ON research_reviews
FOR EACH ROW
EXECUTE FUNCTION research_reviews_guard_decided_paper();

CREATE TRIGGER research_papers_status_transition_trg
BEFORE INSERT OR UPDATE OF status ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_guard_status_transition();

CREATE TRIGGER research_papers_elo_award_guard_trg
BEFORE UPDATE OF elo_award, elo_awarded, elo_awarded_at ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_guard_elo_award();
