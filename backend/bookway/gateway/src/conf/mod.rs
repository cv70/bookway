use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
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
            growth_url: grpc_url("GROWTH_GRPC_URL", "http://127.0.0.1:8081"),
            bbs_feed_url: grpc_url("BBS_FEED_GRPC_URL", "http://127.0.0.1:8088"),
            bbs_link_url: grpc_url("BBS_LINK_GRPC_URL", "http://127.0.0.1:18004"),
            search_main_url: grpc_url("SEARCH_MAIN_GRPC_URL", "http://127.0.0.1:8090"),
            bbs_url: grpc_url("BBS_GRPC_URL", "http://127.0.0.1:18002"),
            comment_url: grpc_url("COMMENT_GRPC_URL", "http://127.0.0.1:18006"),
            like_status_url: grpc_url("LIKE_STATUS_GRPC_URL", "http://127.0.0.1:18007"),
            user_event_url: grpc_url("USER_EVENT_GRPC_URL", "http://127.0.0.1:18089"),
            media_url: grpc_url("MEDIA_GRPC_URL", "http://127.0.0.1:18091"),
        })
    }
}

fn grpc_url(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
