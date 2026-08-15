-- Account owns editable public profile data. Authentication credentials stay
-- outside this database and are verified by Gateway before any account RPC.
CREATE TABLE IF NOT EXISTS account_profiles (
    user_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    avatar_url TEXT NOT NULL DEFAULT '',
    bio TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_account_profiles_updated_at
    ON account_profiles (updated_at DESC);
