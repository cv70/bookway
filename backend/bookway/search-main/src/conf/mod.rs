use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_search_url: String,
    pub(crate) bbs_link_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("SEARCH_MAIN_ADDR", "127.0.0.1:8090")?,
            bbs_search_url: env::var("BBS_SEARCH_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8085".to_string()),
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
        })
    }
}
