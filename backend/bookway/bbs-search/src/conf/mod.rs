use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    pub(crate) opensearch_url: Option<String>,
    pub(crate) opensearch_index: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_SEARCH_ADDR", "127.0.0.1:8085")?,
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            opensearch_url: env::var("OPENSEARCH_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            opensearch_index: env::var("OPENSEARCH_INDEX")
                .unwrap_or_else(|_| "bookway-content-v1".to_string()),
        })
    }
}
