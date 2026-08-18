-- Unified Search pulls public knowledge resources through Search Main.  Keep a
-- compact searchable projection on the catalog table so resource recall does
-- not degrade into multi-column scans as the directory grows.
ALTER TABLE public_resources
    ADD COLUMN IF NOT EXISTS search_text TEXT NOT NULL DEFAULT '';

CREATE OR REPLACE FUNCTION refresh_public_resource_search_text()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.search_text := lower(concat_ws(
        ' ',
        NEW.title,
        NEW.summary,
        NEW.provider,
        NEW.citation,
        array_to_string(NEW.topics, ' ')
    ));
    RETURN NEW;
END;
$$;

UPDATE public_resources
SET search_text = lower(concat_ws(
    ' ',
    title,
    summary,
    provider,
    citation,
    array_to_string(topics, ' ')
))
WHERE search_text = '';

DROP TRIGGER IF EXISTS trg_public_resources_search_text ON public_resources;
CREATE TRIGGER trg_public_resources_search_text
BEFORE INSERT OR UPDATE OF title, summary, provider, citation, topics
ON public_resources
FOR EACH ROW
EXECUTE FUNCTION refresh_public_resource_search_text();

CREATE INDEX IF NOT EXISTS idx_public_resources_search_text_trgm
    ON public_resources USING GIN (search_text gin_trgm_ops)
    WHERE status = 'published';

CREATE INDEX IF NOT EXISTS idx_public_resources_kind_published
    ON public_resources (kind, updated_at DESC, id DESC)
    WHERE status = 'published';
