-- Questions remain ordinary moderated content, while answer selection is an
-- immutable-in-audit content payload field owned by BBS Link.
ALTER TABLE content_items
    DROP CONSTRAINT IF EXISTS content_items_content_type_check;

ALTER TABLE content_items
    ADD CONSTRAINT content_items_content_type_check
    CHECK (content_type IN ('note', 'article', 'video', 'route', 'milestone', 'question'));
