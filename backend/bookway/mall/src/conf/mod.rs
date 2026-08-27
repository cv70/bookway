use bookway_runtime::RuntimeError;
use std::net::SocketAddr;
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    pub(crate) knowledge_catalog_url: String,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("MALL_ADDR", "127.0.0.1:8101")?,
            bbs_link_url: std::env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            knowledge_catalog_url: std::env::var("KNOWLEDGE_CATALOG_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8105".to_string()),
        })
    }
}
