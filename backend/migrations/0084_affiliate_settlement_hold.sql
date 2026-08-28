-- Affiliate refund-window hold (the "cooling-off" period). Paid orders now
-- create creator shares as `pending`; a dedicated promotion pass flips them
-- to `eligible` once `eligible_at` (now + MALL_AFFILIATE_HOLD_DAYS) passes.
-- The partial index keeps that promotion scan cheap; reverse handles both
-- eligible and pending rows so an in-window refund voids the share before
-- any payout.
CREATE INDEX IF NOT EXISTS idx_mall_affiliate_settlements_promotion
    ON mall_affiliate_settlements (eligible_at)
    WHERE status = 'pending';
