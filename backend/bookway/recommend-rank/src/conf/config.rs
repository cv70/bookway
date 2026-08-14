use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};

#[derive(Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub model_version: String,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_RANK_ADDR", "127.0.0.1:8096")?,
            model_version: env::var("RECOMMEND_RANK_MODEL_VERSION")
                .unwrap_or_else(|_| "recommend-rank-v2".to_string()),
        })
    }
}
