use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    pub(crate) bbs_url: String,
    pub(crate) like_status_url: String,
    pub(crate) feature_main_url: String,
    pub(crate) rank_main_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_MAIN_ADDR", "127.0.0.1:8083")?,
            bbs_link_url: env::var("BBS_LINK_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8084".to_string()),
            bbs_url: env::var("BBS_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string()),
            like_status_url: env::var("LIKE_STATUS_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8087".to_string()),
            feature_main_url: env::var("FEATURE_MAIN_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8093".to_string()),
            rank_main_url: env::var("RANK_MAIN_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8094".to_string()),
        })
    }
}
