-- Persist the multi-objective estimates that recommend-rank produced for each
-- served item. Without these columns the three objectives are dropped right
-- after ranking, which makes calibration, experiment evaluation, and any
-- future model training impossible (see the recommendation evaluator).
ALTER TABLE feed_exposure_items
    ADD COLUMN p_ctr REAL NOT NULL DEFAULT 0,
    ADD COLUMN p_cvr REAL NOT NULL DEFAULT 0,
    ADD COLUMN p_wegu REAL NOT NULL DEFAULT 0;
