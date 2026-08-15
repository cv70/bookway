use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) mall_url: String,
    pub(crate) inventory_url: String,
    pub(crate) payment_ttl_seconds: u64,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("MALL_ORDER_ADDR", "127.0.0.1:8103")?,
            mall_url: env::var("MALL_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8101".to_string()),
            inventory_url: env::var("MALL_INVENTORY_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8102".to_string()),
            payment_ttl_seconds: env::var("MALL_PAYMENT_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(900),
        })
    }
}
