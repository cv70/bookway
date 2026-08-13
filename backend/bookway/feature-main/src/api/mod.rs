use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod grpc;

#[path = "pb/bookway.feature.main.rs"]
pub mod pb;

pub(crate) use grpc::serve;

#[derive(Deserialize)]
pub(crate) struct FeatureRequest {
    pub(crate) user_id: String,
    pub(crate) content_ids: Vec<String>,
}
#[derive(Serialize)]
pub(crate) struct FeatureResponse {
    pub(crate) user_id: String,
    pub(crate) model_version: String,
    pub(crate) features: HashMap<String, f64>,
}
