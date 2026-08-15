use std::env;
use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) recommend_main_url: String,
    pub(crate) search_main_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("USER_EVENT_ADDR", "127.0.0.1:8089")?,
            grpc_addr: bookway_runtime::listen_addr("USER_EVENT_GRPC_ADDR", "127.0.0.1:18089")?,
            recommend_main_url: env::var("RECOMMEND_MAIN_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8083".to_string()),
            search_main_url: env::var("SEARCH_MAIN_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string()),
        })
    }
}
