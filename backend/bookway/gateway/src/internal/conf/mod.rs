use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) growth_url: String,
    pub(crate) bbs_feed_url: String,
    pub(crate) bbs_link_url: String,
    pub(crate) search_main_url: String,
    pub(crate) bbs_url: String,
    pub(crate) comment_url: String,
    pub(crate) like_status_url: String,
    pub(crate) user_event_url: String,
    pub(crate) media_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("GATEWAY_ADDR", "0.0.0.0:8080")?,
            growth_url: env::var("GROWTH_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".to_string()),
            bbs_feed_url: env::var("BBS_FEED_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8088".to_string()),
            bbs_link_url: env::var("BBS_LINK_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8084".to_string()),
            search_main_url: env::var("SEARCH_MAIN_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string()),
            bbs_url: env::var("BBS_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string()),
            comment_url: env::var("COMMENT_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8086".to_string()),
            like_status_url: env::var("LIKE_STATUS_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8087".to_string()),
            user_event_url: env::var("USER_EVENT_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8089".to_string()),
            media_url: env::var("MEDIA_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8091".to_string()),
        })
    }
}
