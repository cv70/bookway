use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_CENTER_ADDR", "127.0.0.1:8097")?,
            bbs_link_url: std::env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
        })
    }
}
