use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) recommend_main_url: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_FEED_ADDR", "127.0.0.1:8088")?,
            recommend_main_url: env::var("RECOMMEND_MAIN_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8083".to_string()),
        })
    }
}
