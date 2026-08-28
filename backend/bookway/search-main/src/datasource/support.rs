use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bookway_bbs_search_api::pb;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub(crate) struct QueryRewriteRule {
    pub(crate) trigger: String,
    pub(crate) expansion_terms: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryRewriteDictionary {
    pub(crate) version: String,
    pub(crate) rules: Vec<QueryRewriteRule>,
}

#[derive(Debug, Error)]
pub(crate) enum QueryRewriteError {
    #[error("query rewrite configuration storage failed: {0}")]
    Storage(String),
}

#[async_trait]
pub(crate) trait QueryRewriteDao: Send + Sync {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError>;
}

pub(crate) type SharedQueryRewriteDao = Arc<dyn QueryRewriteDao>;

pub(crate) fn builtin_query_rewrite_dictionary() -> QueryRewriteDictionary {
    QueryRewriteDictionary {
        version: "builtin-v1".to_string(),
        rules: vec![
            QueryRewriteRule {
                trigger: "跑步".to_string(),
                expansion_terms: vec!["慢跑".to_string(), "晨跑".to_string(), "夜跑".to_string()],
            },
            QueryRewriteRule {
                trigger: "阅读".to_string(),
                expansion_terms: vec![
                    "读书".to_string(),
                    "书单".to_string(),
                    "主题阅读".to_string(),
                ],
            },
            QueryRewriteRule {
                trigger: "睡眠".to_string(),
                expansion_terms: vec![
                    "早睡".to_string(),
                    "作息".to_string(),
                    "睡眠修复".to_string(),
                ],
            },
            QueryRewriteRule {
                trigger: "冥想".to_string(),
                expansion_terms: vec!["正念".to_string(), "呼吸".to_string(), "静坐".to_string()],
            },
            QueryRewriteRule {
                trigger: "旅行".to_string(),
                expansion_terms: vec![
                    "徒步".to_string(),
                    "城市漫游".to_string(),
                    "出行".to_string(),
                ],
            },
            QueryRewriteRule {
                trigger: "徒步".to_string(),
                expansion_terms: vec!["登山".to_string(), "步道".to_string(), "远足".to_string()],
            },
            // Route action nodes and their equipment are first-class search
            // vocabulary. Keep these expansions bounded and versioned so a
            // semantic improvement can be rolled back with the dictionary.
            QueryRewriteRule {
                trigger: "登山鞋".to_string(),
                expansion_terms: vec![
                    "徒步鞋".to_string(),
                    "越野鞋".to_string(),
                    "防滑鞋".to_string(),
                ],
            },
            QueryRewriteRule {
                trigger: "头盔".to_string(),
                expansion_terms: vec!["骑行头盔".to_string(), "安全帽".to_string()],
            },
            QueryRewriteRule {
                trigger: "行动节点".to_string(),
                expansion_terms: vec!["行动".to_string(), "步骤".to_string(), "任务".to_string()],
            },
        ],
    }
}

#[derive(Debug, Error)]
pub(crate) enum SearchSessionError {
    #[error("search pipeline session storage failed: {0}")]
    Storage(String),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RecallState {
    pub(crate) source: RecallSource,
    pub(crate) query: String,
    pub(crate) source_cursor: Option<String>,
    pub(crate) exhausted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecallSource {
    Bbs,
    Resource,
    /// One-shot vector recall over the semantically embedded index. The lane
    /// has no cursor: one bounded batch, then exhausted.
    Semantic,
}

/// The main-search session mixes independently paged recalls without exposing
/// upstream cursor tokens to clients. Source cursors remain owned by bbs-search.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SearchPipelineSession {
    pub(crate) query_fingerprint: u64,
    pub(crate) query_rewrite_version: String,
    pub(crate) recalls: Vec<RecallState>,
    pub(crate) pending: Vec<pb::SearchResult>,
    pub(crate) seen_result_ids: HashSet<String>,
    pub(crate) delivered_count: usize,
    pub(crate) source_total_estimate: usize,
    pub(crate) degraded: bool,
}

#[async_trait]
pub(crate) trait SearchSessionStore: Send + Sync {
    async fn create(&self, session: SearchPipelineSession) -> Result<String, SearchSessionError>;
    async fn load(&self, id: &str) -> Result<Option<SearchPipelineSession>, SearchSessionError>;
    /// Returns false when a session expires between load and save.
    async fn save(
        &self,
        id: &str,
        session: SearchPipelineSession,
    ) -> Result<bool, SearchSessionError>;
    async fn delete(&self, id: &str) -> Result<(), SearchSessionError>;
}

const SEARCH_MAIN_SESSION_TTL: Duration = Duration::from_secs(5 * 60);
const SEARCH_EXPOSURE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MAX_MEMORY_SEARCH_EXPOSURES: usize = 20_000;
const SEARCH_EXPOSURE_CLEANUP_BATCH_SIZE: i64 = 1_000;

#[derive(Clone, Debug)]
pub(crate) struct SearchExposure {
    pub(crate) request_id: String,
    pub(crate) user_id: String,
    pub(crate) session_id: String,
    pub(crate) query_hash: String,
    pub(crate) query_rewrite_version: String,
    pub(crate) degraded: bool,
    pub(crate) items: Vec<SearchExposureItem>,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchExposureItem {
    pub(crate) position: usize,
    pub(crate) result_id: String,
    pub(crate) result_type: String,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchAttribution {
    pub(crate) request_id: String,
    pub(crate) session_id: String,
    pub(crate) result_id: String,
    pub(crate) position: u32,
}

#[derive(Debug, Error)]
pub(crate) enum SearchExposureError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("attribution position exceeds PostgreSQL integer range")]
    PositionOutOfRange,
}

#[async_trait]
pub(crate) trait SearchExposureStore: Send + Sync {
    async fn record(&self, exposure: SearchExposure) -> Result<(), SearchExposureError>;
    async fn validate(
        &self,
        user_id: &str,
        attributions: &[SearchAttribution],
    ) -> Result<Vec<bool>, SearchExposureError>;
}

pub(crate) type SharedSearchExposureStore = Arc<dyn SearchExposureStore>;

#[cfg(test)]
mod tests {
    use super::{
        MemorySearchExposureStore, SearchAttribution, SearchExposure, SearchExposureItem,
        SearchExposureStore,
    };

    #[tokio::test]
    async fn memory_search_attribution_binds_viewer_session_result_and_position() {
        let store = MemorySearchExposureStore::default();
        store
            .record(SearchExposure {
                request_id: "request-1".to_string(),
                user_id: "user-1".to_string(),
                session_id: "session-1".to_string(),
                query_hash: "hash".to_string(),
                query_rewrite_version: "builtin-v1".to_string(),
                degraded: false,
                items: vec![SearchExposureItem {
                    position: 2,
                    result_id: "post-1".to_string(),
                    result_type: "SEARCH_RESULT_TYPE_POST".to_string(),
                }],
            })
            .await
            .expect("memory exposure record should succeed");

        let valid = store
            .validate(
                "user-1",
                &[
                    SearchAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-1".to_string(),
                        result_id: "post-1".to_string(),
                        position: 2,
                    },
                    SearchAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-2".to_string(),
                        result_id: "post-1".to_string(),
                        position: 2,
                    },
                    SearchAttribution {
                        request_id: "request-1".to_string(),
                        session_id: "session-1".to_string(),
                        result_id: "post-1".to_string(),
                        position: 1,
                    },
                ],
            )
            .await
            .expect("memory search validation should succeed");

        assert_eq!(valid, [true, false, false]);
    }
}

#[path = "memory_query_rewrite_dao.rs"]
mod memory_query_rewrite_dao;
pub(crate) use memory_query_rewrite_dao::MemoryQueryRewriteDao;
#[path = "postgres_query_rewrite_dao.rs"]
mod postgres_query_rewrite_dao;
pub(crate) use postgres_query_rewrite_dao::PostgresQueryRewriteDao;
#[path = "memory_search_session_store.rs"]
mod memory_search_session_store;
pub(crate) use memory_search_session_store::MemorySearchSessionStore;
#[path = "postgres_search_session_store.rs"]
mod postgres_search_session_store;
pub(crate) use postgres_search_session_store::PostgresSearchSessionStore;
#[path = "memory_search_exposure_store.rs"]
mod memory_search_exposure_store;
pub(crate) use memory_search_exposure_store::MemorySearchExposureStore;
#[path = "postgres_search_exposure_store.rs"]
mod postgres_search_exposure_store;
pub(crate) use postgres_search_exposure_store::PostgresSearchExposureStore;
