use bookway_runtime::RuntimeError;
use std::{env, net::SocketAddr};
#[derive(Clone)]
pub struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) model_version: String,
    /// Blend observed click evidence into the serving inputs (`ecpm-v3`).
    /// `false` reproduces the static `ecpm-v2` inputs exactly — the one-key
    /// rollback if calibration misbehaves.
    pub(crate) calibrated: bool,
}
impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("AD_RANK_ADDR", "127.0.0.1:8099")?,
            model_version: env::var("AD_RANK_MODEL_VERSION")
                .unwrap_or_else(|_| "ecpm-v3".to_string()),
            calibrated: !matches!(
                env::var("AD_RANK_CALIBRATION")
                    .unwrap_or_else(|_| "true".to_string())
                    .to_lowercase()
                    .as_str(),
                "0" | "false" | "off"
            ),
        })
    }
}
