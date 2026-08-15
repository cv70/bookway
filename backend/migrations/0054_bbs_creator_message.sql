-- Creator positioning is intentionally separate from account identity and
-- content facts. Account owns display name/avatar; bbs-link owns content.
CREATE TABLE IF NOT EXISTS creator_profiles (
    user_id TEXT PRIMARY KEY,
    handle TEXT NOT NULL,
    headline TEXT NOT NULL DEFAULT '',
    introduction TEXT NOT NULL DEFAULT '',
    cover_url TEXT NOT NULL DEFAULT '',
    specialties TEXT[] NOT NULL DEFAULT '{}',
    featured_content_ids TEXT[] NOT NULL DEFAULT '{}',
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'paused')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (char_length(handle) BETWEEN 3 AND 32),
    CHECK (handle = lower(handle)),
    CHECK (headline = btrim(headline)),
    CHECK (introduction = btrim(introduction))
);

CREATE UNIQUE INDEX IF NOT EXISTS creator_profiles_handle_lower_key
    ON creator_profiles (lower(handle));
CREATE INDEX IF NOT EXISTS idx_creator_profiles_updated
    ON creator_profiles (updated_at DESC, user_id DESC);
CREATE INDEX IF NOT EXISTS idx_creator_profiles_specialties
    ON creator_profiles USING GIN (specialties);

-- A one-to-one conversation ID is deterministic in the service, while the
-- ordered pair constraint protects against duplicate rows under concurrency.
CREATE TABLE IF NOT EXISTS direct_conversations (
    id TEXT PRIMARY KEY,
    participant_one_id TEXT NOT NULL,
    participant_two_id TEXT NOT NULL,
    last_message_id TEXT,
    last_message_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (participant_one_id, participant_two_id),
    CHECK ((participant_one_id COLLATE "C") < (participant_two_id COLLATE "C"))
);

CREATE TABLE IF NOT EXISTS direct_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES direct_conversations (id) ON DELETE CASCADE,
    sender_user_id TEXT NOT NULL,
    recipient_user_id TEXT NOT NULL,
    client_message_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('text')),
    body TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    read_at TIMESTAMPTZ,
    UNIQUE (sender_user_id, client_message_id),
    CHECK (sender_user_id <> recipient_user_id)
);

CREATE INDEX IF NOT EXISTS idx_direct_messages_conversation_created
    ON direct_messages (conversation_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_direct_messages_unread
    ON direct_messages (conversation_id, recipient_user_id, created_at DESC, id DESC)
    WHERE read_at IS NULL;

CREATE TABLE IF NOT EXISTS direct_message_preferences (
    user_id TEXT PRIMARY KEY,
    allow_direct_messages BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_direct_conversations_participant_one_recent
    ON direct_conversations (participant_one_id, last_message_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_direct_conversations_participant_two_recent
    ON direct_conversations (participant_two_id, last_message_at DESC, id DESC);
