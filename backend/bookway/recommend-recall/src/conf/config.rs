use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceBlend {
    ScoreV1,
    BalancedV1,
}

impl SourceBlend {
    pub fn version(self) -> &'static str {
        match self {
            Self::ScoreV1 => "score-v1",
            Self::BalancedV1 => "balanced-v1",
        }
    }

    fn from_env() -> Result<Self, RuntimeError> {
        let value = env::var("RECALL_SOURCE_BLEND").unwrap_or_else(|_| "balanced-v1".to_string());
        match value.as_str() {
            "score-v1" => Ok(Self::ScoreV1),
            "balanced-v1" => Ok(Self::BalancedV1),
            _ => Err(RuntimeError::InvalidSetting {
                key: "RECALL_SOURCE_BLEND".to_string(),
                value,
            }),
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub bbs_link_url: String,
    pub max_candidates: usize,
    pub source_blend: SourceBlend,
}

impl Config {
    pub fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("RECOMMEND_RECALL_ADDR", "127.0.0.1:8095")?,
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            max_candidates: env::var("RECALL_MAX_CANDIDATES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(500),
            source_blend: SourceBlend::from_env()?,
        })
    }
}
