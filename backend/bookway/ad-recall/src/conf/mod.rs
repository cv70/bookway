use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};

#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) center_url: String,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_RECALL_ADDR", "127.0.0.1:8098")?,
            center_url: env::var("AD_CENTER_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8097".to_string()),
        })
    }
}
