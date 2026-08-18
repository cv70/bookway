use bookway_runtime::RuntimeError;
use std::net::SocketAddr;
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) reservation_ttl_seconds: u64,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("MALL_INVENTORY_ADDR", "127.0.0.1:8102")?,
            reservation_ttl_seconds: std::env::var("MALL_RESERVATION_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(900),
        })
    }
}
