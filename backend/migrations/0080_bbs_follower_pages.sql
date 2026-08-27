-- Follower pages walk one user's inbound follow edges newest-first and resume
-- from the previous page's (followed_at, follower_id). The generic 0002 target
-- index cannot order by created_at, so every keyset page would sort. This
-- covering index turns each page into a single ordered index scan.
CREATE INDEX IF NOT EXISTS idx_social_edges_followers
    ON social_edges (target_user_id, created_at DESC, source_user_id DESC)
    WHERE edge_type = 'follow' AND deleted_at IS NULL;
