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
            listen_addr: bookway_runtime::listen_addr("COMMENT_ADDR", "127.0.0.1:8086")?,
            grpc_addr: bookway_runtime::listen_addr("COMMENT_GRPC_ADDR", "127.0.0.1:18006")?,
            content_audit_grpc_url: std::env::var("CONTENT_AUDIT_GRPC_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }
}
