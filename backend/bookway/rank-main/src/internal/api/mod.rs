use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Candidate {
    pub(crate) content_id: String,
    pub(crate) recall_score: f64,
    pub(crate) quality_score: f64,
    pub(crate) freshness: f64,
}

#[derive(Deserialize)]
pub(crate) struct RankRequest {
    pub(crate) user_id: String,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) features: serde_json::Value,
}

#[derive(Serialize)]
pub(crate) struct RankedItem {
    pub(crate) content_id: String,
    pub(crate) score: f64,
    pub(crate) model_version: String,
    pub(crate) experiment_bucket: String,
}
