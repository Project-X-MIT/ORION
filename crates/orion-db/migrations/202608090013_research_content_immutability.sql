-- Proposed to Div as a forward-only follow-up to DB-04.
-- Once a paper leaves draft, content changes require a new paper/revision ID.

CREATE OR REPLACE FUNCTION research_papers_guard_content_immutability()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'draft'
       AND (
           NEW.title IS DISTINCT FROM OLD.title
           OR NEW.abstract IS DISTINCT FROM OLD.abstract
           OR NEW.content IS DISTINCT FROM OLD.content
       ) THEN
        RAISE EXCEPTION 'research content is immutable after draft; create a new revision'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER research_papers_content_immutability_trg
BEFORE UPDATE OF title, abstract, content ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_guard_content_immutability();
