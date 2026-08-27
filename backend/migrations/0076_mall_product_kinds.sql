-- Catalogue segmentation for the mall: knowledge products (courses and
-- resource packs) bind a knowledge-catalog public resource; physical goods
-- are unaffected. The cross-service binding is validated in the mall domain
-- layer against KnowledgeCatalog.Get — no foreign key across service tables.
ALTER TABLE mall_products
    ADD COLUMN IF NOT EXISTS product_kind TEXT NOT NULL DEFAULT 'physical'
    CHECK (product_kind IN ('physical', 'course', 'resource_pack')),
    ADD COLUMN IF NOT EXISTS course_resource_id TEXT NOT NULL DEFAULT '';
