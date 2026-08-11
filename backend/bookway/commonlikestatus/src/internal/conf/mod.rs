use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("LIKE_STATUS_ADDR", "127.0.0.1:8087")?,
        })
    }
}
