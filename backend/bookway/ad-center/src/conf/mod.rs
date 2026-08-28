use std::net::SocketAddr;

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    /// Even-delivery pacing: campaigns whose spend already outruns the linear
    /// day budget (plus a fixed catch-up headroom) sit out new decisions until
    /// the day catches up. On by default; RecordEvent's hard budget cap stays
    /// authoritative regardless of this flag.
    pub(crate) pacing_enabled: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        let pacing_enabled = std::env::var("AD_CENTER_PACING_ENABLED")
            .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
            .unwrap_or(true);
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_CENTER_ADDR", "127.0.0.1:8097")?,
            bbs_link_url: std::env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            pacing_enabled,
        })
    }
}
