-- Research content is the version evaluated and publicly cached. Once a
-- paper leaves draft, its title, abstract, and content must not change under
-- any writer, including direct SQL callers.
CREATE OR REPLACE FUNCTION research_papers_guard_immutable_content()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'draft' AND (
        NEW.title IS DISTINCT FROM OLD.title
        OR NEW.abstract IS DISTINCT FROM OLD.abstract
        OR NEW.content IS DISTINCT FROM OLD.content
    ) THEN
        RAISE EXCEPTION 'research content is immutable after submission'
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER research_papers_content_immutability_trg
BEFORE UPDATE OF title, abstract, content ON research_papers
FOR EACH ROW
EXECUTE FUNCTION research_papers_guard_immutable_content();
