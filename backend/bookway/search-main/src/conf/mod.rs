use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_search_url: String,
    pub(crate) bbs_link_url: String,
    pub(crate) bbs_url: String,
    pub(crate) knowledge_catalog_url: String,
    pub(crate) feature_main_url: String,
    pub(crate) ad_main_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("SEARCH_MAIN_ADDR", "127.0.0.1:8090")?,
            bbs_search_url: env::var("BBS_SEARCH_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8085".to_string()),
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            bbs_url: env::var("BBS_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18002".to_string()),
            knowledge_catalog_url: env::var("KNOWLEDGE_CATALOG_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8105".to_string()),
            feature_main_url: env::var("FEATURE_MAIN_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8093".to_string()),
            ad_main_url: env::var("AD_MAIN_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8100".to_string()),
        })
    }
}
