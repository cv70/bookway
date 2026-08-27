use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr, time::Duration};
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) recall_url: String,
    pub(crate) rank_url: String,
    pub(crate) center_url: String,
    pub(crate) max_decisions: usize,
    /// Minimum interval between decisions that carry ads for one user.
    /// `None` (the default) disables pacing entirely; set
    /// `AD_MAIN_IMPRESSION_COOLDOWN_MS` to opt in. A Redis outage or an
    /// unset `REDIS_URL` disables it at runtime too — this is a serving
    /// experience throttle, not a delivery guarantee.
    pub(crate) impression_cooldown: Option<Duration>,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_MAIN_ADDR", "127.0.0.1:8100")?,
            recall_url: env::var("AD_RECALL_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8098".to_string()),
            rank_url: env::var("AD_RANK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8099".to_string()),
            center_url: env::var("AD_CENTER_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8097".to_string()),
            max_decisions: env::var("AD_MAX_DECISIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3),
            impression_cooldown: env::var("AD_MAIN_IMPRESSION_COOLDOWN_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|millis| *millis > 0)
                .map(Duration::from_millis),
        })
    }
}
