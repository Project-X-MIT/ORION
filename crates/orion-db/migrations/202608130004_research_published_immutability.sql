-- Published research is an immutable public record. Keep this guard in a new
-- forward-only migration so already-applied migrations remain untouched.
CREATE OR REPLACE FUNCTION research_papers_guard_published_content()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'published' AND NEW.content IS DISTINCT FROM OLD.content THEN
        RAISE EXCEPTION 'published research content is immutable'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER research_papers_published_content_guard_trg
BEFORE UPDATE OF content ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_guard_published_content();
