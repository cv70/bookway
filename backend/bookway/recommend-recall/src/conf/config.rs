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

/// Upstream endpoints for the semantic recall lane. Both must be configured;
/// otherwise the lane is simply not registered and the remaining recall
/// sources serve the feed unchanged (the same contract knowledge-catalog
/// applies to `RAG_EMBEDDING_ENDPOINT`).
#[derive(Clone)]
pub struct SemanticConfig {
    pub bbs_search_url: String,
    pub knowledge_catalog_url: String,
}

impl SemanticConfig {
    fn from_values(
        bbs_search_url: String,
        knowledge_catalog_url: String,
    ) -> Option<Self> {
        if bbs_search_url.trim().is_empty() || knowledge_catalog_url.trim().is_empty() {
            return None;
        }
        Some(Self {
            bbs_search_url,
            knowledge_catalog_url,
        })
    }

    fn from_env() -> Option<Self> {
        Self::from_values(
            env::var("RECALL_SEMANTIC_BBS_SEARCH_URL").unwrap_or_default(),
            env::var("RECALL_SEMANTIC_KNOWLEDGE_CATALOG_URL").unwrap_or_default(),
        )
    }
}

#[derive(Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub bbs_link_url: String,
    pub max_candidates: usize,
    pub source_blend: SourceBlend,
    pub semantic: Option<SemanticConfig>,
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
            semantic: SemanticConfig::from_env(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticConfig;

    #[test]
    fn semantic_lane_is_registered_only_when_both_endpoints_are_configured() {
        assert!(SemanticConfig::from_values(String::new(), String::new()).is_none());
        assert!(
            SemanticConfig::from_values("http://127.0.0.1:8085".to_string(), String::new())
                .is_none()
        );
        assert!(
            SemanticConfig::from_values(" ".to_string(), "http://127.0.0.1:8105".to_string())
                .is_none()
        );

        let semantic = SemanticConfig::from_values(
            "http://127.0.0.1:8085".to_string(),
            "http://127.0.0.1:8105".to_string(),
        )
        .expect("both endpoints registered");
        assert_eq!(semantic.bbs_search_url, "http://127.0.0.1:8085");
        assert_eq!(semantic.knowledge_catalog_url, "http://127.0.0.1:8105");
    }
}
