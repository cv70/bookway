-- A weekly review is a user-confirmed historical snapshot. The period is the
-- natural upsert key; users may refine their reflection without rewriting the
-- metrics and suggestions that were shown when they first confirmed it.
CREATE TABLE IF NOT EXISTS weekly_reviews (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, period_start, period_end),
    CHECK (period_start <= period_end)
);

CREATE INDEX IF NOT EXISTS idx_weekly_reviews_user_period
    ON weekly_reviews (user_id, period_start DESC);
