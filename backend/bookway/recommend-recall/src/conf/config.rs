use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub bbs_link_url: String,
    pub max_candidates: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_RECALL_ADDR", "127.0.0.1:8095")?,
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            max_candidates: env::var("RECALL_MAX_CANDIDATES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(500),
        })
    }
}
