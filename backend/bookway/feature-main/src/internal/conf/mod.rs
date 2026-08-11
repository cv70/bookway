use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) model_version: String,
}
impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("FEATURE_MAIN_ADDR", "127.0.0.1:8093")?,
            model_version: env::var("FEATURE_MODEL_VERSION")
                .unwrap_or_else(|_| "heuristic-v1".to_string()),
        })
    }
}
