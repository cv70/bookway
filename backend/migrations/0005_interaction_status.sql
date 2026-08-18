CREATE TABLE IF NOT EXISTS reactions (
    user_id TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    reaction_type TEXT NOT NULL CHECK (reaction_type IN ('like', 'bookmark')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, target_type, target_id, reaction_type)
);
CREATE INDEX IF NOT EXISTS idx_reactions_target ON reactions (target_type, target_id, reaction_type)
    WHERE deleted_at IS NULL;
