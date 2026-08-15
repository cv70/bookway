use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_CENTER_ADDR", "127.0.0.1:8097")?,
        })
    }
}
