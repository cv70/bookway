-- Keep comment ancestry explicit so reply creation can enforce a fixed limit
-- without recursive reads on the serving path. The CTE also repairs rows
-- written before this column existed. Malformed cyclic legacy rows are marked
-- beyond the serving limit so they cannot accept more replies.
ALTER TABLE comments
    ADD COLUMN IF NOT EXISTS depth SMALLINT;

WITH RECURSIVE comment_depths AS (
    SELECT id, parent_id, 0::INTEGER AS depth, ARRAY[id] AS path
    FROM comments
    WHERE parent_id IS NULL

    UNION ALL

    SELECT child.id, child.parent_id, parent.depth + 1, parent.path || child.id
    FROM comments AS child
    JOIN comment_depths AS parent ON child.parent_id = parent.id
    WHERE NOT child.id = ANY(parent.path)
)
UPDATE comments AS comment
SET depth = LEAST(calculated.depth, 32767)::SMALLINT
FROM comment_depths AS calculated
WHERE comment.id = calculated.id;

UPDATE comments
SET depth = 32767
WHERE depth IS NULL;

ALTER TABLE comments
    ALTER COLUMN depth SET DEFAULT 0,
    ALTER COLUMN depth SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'comments_depth_nonnegative'
    ) THEN
        ALTER TABLE comments
            ADD CONSTRAINT comments_depth_nonnegative CHECK (depth >= 0);
    END IF;
END $$;
