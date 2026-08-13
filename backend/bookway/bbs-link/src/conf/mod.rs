use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) content_audit_grpc_url: Option<String>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_LINK_ADDR", "127.0.0.1:8084")?,
            grpc_addr: bookway_runtime::listen_addr("BBS_LINK_GRPC_ADDR", "127.0.0.1:18004")?,
            content_audit_grpc_url: std::env::var("CONTENT_AUDIT_GRPC_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }
}
