use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) bbs_grpc_url: String,
    pub(crate) content_audit_grpc_url: Option<String>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_MESSAGE_ADDR", "127.0.0.1:8106")?,
            grpc_addr: bookway_runtime::listen_addr("BBS_MESSAGE_GRPC_ADDR", "127.0.0.1:18106")?,
            bbs_grpc_url: std::env::var("BBS_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18002".to_string()),
            content_audit_grpc_url: std::env::var("CONTENT_AUDIT_GRPC_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        })
    }
}
