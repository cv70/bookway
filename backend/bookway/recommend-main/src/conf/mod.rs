use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_url: String,
    pub(crate) interaction_status_url: String,
    pub(crate) feature_main_url: String,
    pub(crate) recommend_rank_url: String,
    pub(crate) recommend_recall_url: String,
    pub(crate) ad_main_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_MAIN_ADDR", "127.0.0.1:8083")?,
            bbs_url: grpc_url("BBS_GRPC_URL", "http://127.0.0.1:18002"),
            interaction_status_url: grpc_url(
                "INTERACTION_STATUS_GRPC_URL",
                "http://127.0.0.1:18007",
            ),
            feature_main_url: grpc_url("FEATURE_MAIN_GRPC_URL", "http://127.0.0.1:8093"),
            recommend_rank_url: env::var("RECOMMEND_RANK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8096".to_string()),
            recommend_recall_url: env::var("RECOMMEND_RECALL_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8095".to_string()),
            ad_main_url: grpc_url("AD_MAIN_GRPC_URL", "http://127.0.0.1:8100"),
        })
    }
}

fn grpc_url(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
