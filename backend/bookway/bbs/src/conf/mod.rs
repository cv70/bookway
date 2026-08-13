use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) grpc_addr: SocketAddr,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_ADDR", "127.0.0.1:8082")?,
            grpc_addr: bookway_runtime::listen_addr("BBS_GRPC_ADDR", "127.0.0.1:18002")?,
        })
    }
}
