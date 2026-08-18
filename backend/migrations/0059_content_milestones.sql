-- A milestone is an independently moderated public content item that records
-- progress on a public route. Its detailed structure remains in the content
-- payload; this constraint keeps the queryable type column in sync.
ALTER TABLE content_items
    DROP CONSTRAINT IF EXISTS content_items_content_type_check;

ALTER TABLE content_items
    ADD CONSTRAINT content_items_content_type_check
    CHECK (content_type IN ('note', 'article', 'video', 'route', 'milestone'));
