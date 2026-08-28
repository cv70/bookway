-- Training on serving-time features is the only leak-free training input:
-- re-reading feature-main at training time would aggregate behavior that
-- happened after the impression. Every ranked item stores the named feature
-- values recommend-rank used (empty object when ranking ran without a model).
ALTER TABLE feed_exposure_items
    ADD COLUMN feature_snapshot JSONB NOT NULL DEFAULT '{}';
