use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) model_version: String,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_RANK_ADDR", "127.0.0.1:8099")?,
            model_version: env::var("AD_RANK_MODEL_VERSION")
                .unwrap_or_else(|_| "ad-heuristic-v1".to_string()),
        })
    }
}
