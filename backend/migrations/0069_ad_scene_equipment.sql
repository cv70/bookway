ALTER TABLE ad_campaigns
    ADD COLUMN scene_equipment TEXT NOT NULL;

CREATE INDEX idx_ad_campaigns_scene_equipment
    ON ad_campaigns (route_id, action_node_id, scene_equipment, status);
