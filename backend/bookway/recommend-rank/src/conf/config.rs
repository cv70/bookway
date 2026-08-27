use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};

#[derive(Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub model_version: String,
    /// Endpoint reserved for the standalone model-serving deployment. Empty
    /// keeps ranking on the deterministic heuristic predictor.
    pub model_endpoint: Option<String>,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        let model_endpoint = env::var("RECOMMEND_RANK_MODEL_ENDPOINT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_RANK_ADDR", "127.0.0.1:8096")?,
            model_version: env::var("RECOMMEND_RANK_MODEL_VERSION")
                .unwrap_or_else(|_| "recommend-rank-v2".to_string()),
            model_endpoint,
        })
    }
}
