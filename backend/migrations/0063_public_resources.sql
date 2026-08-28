CREATE TABLE IF NOT EXISTS public_resources (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('book', 'course', 'tool', 'article', 'podcast')),
    provider TEXT NOT NULL,
    summary TEXT NOT NULL,
    url TEXT NOT NULL,
    license TEXT NOT NULL,
    version TEXT NOT NULL,
    citation TEXT NOT NULL,
    topics TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('published', 'archived')),
    published_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_public_resources_published
    ON public_resources (status, updated_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_public_resources_topics
    ON public_resources USING GIN (topics);

-- The canonical URL is the catalog's identity anchor: admin upserts that hit
-- an existing URL update that entry instead of creating a duplicate.
CREATE UNIQUE INDEX IF NOT EXISTS ux_public_resources_url
    ON public_resources (url);
