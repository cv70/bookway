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
pub(crate) trait QueryRewriteRepository: Send + Sync {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError>;
}

pub(crate) type SharedQueryRewriteRepository = Arc<dyn QueryRewriteRepository>;

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
        ],
    }
}

pub(crate) struct MemoryQueryRewriteRepository;

#[async_trait]
impl QueryRewriteRepository for MemoryQueryRewriteRepository {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError> {
        Ok(Some(builtin_query_rewrite_dictionary()))
    }
}

pub(crate) struct PostgresQueryRewriteRepository {
    pool: sqlx::PgPool,
}

impl PostgresQueryRewriteRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueryRewriteRepository for PostgresQueryRewriteRepository {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError> {
        let rows = sqlx::query_as::<_, (String, Option<String>, Option<Vec<String>>)>(
            "SELECT active.version, rule.trigger, rule.expansion_terms FROM search_query_rewrite_active AS active INNER JOIN search_query_rewrite_versions AS version ON version.version = active.version AND version.status = 'ready' LEFT JOIN search_query_rewrite_rules AS rule ON rule.version = active.version ORDER BY rule.trigger",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| QueryRewriteError::Storage(error.to_string()))?;
        let Some((version, _, _)) = rows.first() else {
            return Ok(None);
        };
        let version = version.clone();
        let rules = rows
            .into_iter()
            .filter_map(|(_, trigger, expansion_terms)| {
                Some(QueryRewriteRule {
                    trigger: trigger?,
                    expansion_terms: expansion_terms?,
                })
            })
            .collect();
        Ok(Some(QueryRewriteDictionary { version, rules }))
    }
}

#[derive(Debug, Error)]
pub(crate) enum SearchSessionError {
    #[error("search pipeline session storage failed: {0}")]
    Storage(String),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RecallState {
    #[serde(default = "default_recall_source")]
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
}

fn default_recall_source() -> RecallSource {
    RecallSource::Bbs
}

/// The main-search session mixes independently paged recalls without exposing
/// upstream cursor tokens to clients. Source cursors remain owned by bbs-search.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SearchPipelineSession {
    pub(crate) query_fingerprint: u64,
    #[serde(default = "legacy_query_rewrite_version")]
    pub(crate) query_rewrite_version: String,
    pub(crate) recalls: Vec<RecallState>,
    pub(crate) pending: Vec<pb::SearchResult>,
    pub(crate) seen_result_ids: HashSet<String>,
    pub(crate) delivered_count: usize,
    pub(crate) source_total_estimate: usize,
    pub(crate) degraded: bool,
}

fn legacy_query_rewrite_version() -> String {
    "legacy-unversioned".to_string()
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

#[derive(Default)]
pub(crate) struct MemorySearchSessionStore {
    sessions: RwLock<HashMap<String, (SearchPipelineSession, Instant)>>,
}

#[async_trait]
impl SearchSessionStore for MemorySearchSessionStore {
    async fn create(&self, session: SearchPipelineSession) -> Result<String, SearchSessionError> {
        let id = Uuid::now_v7().to_string();
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        sessions.insert(id.clone(), (session, now + SEARCH_MAIN_SESSION_TTL));
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchPipelineSession>, SearchSessionError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        Ok(sessions.get(id).map(|(session, _)| session.clone()))
    }

    async fn save(
        &self,
        id: &str,
        session: SearchPipelineSession,
    ) -> Result<bool, SearchSessionError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        let Some((stored, expires_at)) = sessions.get_mut(id) else {
            return Ok(false);
        };
        *stored = session;
        *expires_at = now + SEARCH_MAIN_SESSION_TTL;
        Ok(true)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSessionError> {
        self.sessions.write().await.remove(id);
        Ok(())
    }
}

pub(crate) struct PostgresSearchSessionStore {
    pool: sqlx::PgPool,
}

impl PostgresSearchSessionStore {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchSessionStore for PostgresSearchSessionStore {
    async fn create(&self, session: SearchPipelineSession) -> Result<String, SearchSessionError> {
        let id = Uuid::now_v7().to_string();
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        sqlx::query(
            "WITH expired AS (DELETE FROM search_main_sessions WHERE expires_at <= now()) INSERT INTO search_main_sessions (session_id,state,expires_at) VALUES ($1,$2,now() + ($3 * interval '1 second'))",
        )
        .bind(&id)
        .bind(state)
        .bind(SEARCH_MAIN_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchPipelineSession>, SearchSessionError> {
        let state = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT state FROM search_main_sessions WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        state
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| SearchSessionError::Storage(error.to_string()))
    }

    async fn save(
        &self,
        id: &str,
        session: SearchPipelineSession,
    ) -> Result<bool, SearchSessionError> {
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE search_main_sessions SET state = $2, expires_at = now() + ($3 * interval '1 second') WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .bind(state)
        .bind(SEARCH_MAIN_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSessionError> {
        sqlx::query("DELETE FROM search_main_sessions WHERE session_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| SearchSessionError::Storage(error.to_string()))?;
        Ok(())
    }
}

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

#[derive(Default)]
pub(crate) struct MemorySearchExposureStore {
    exposures: RwLock<Vec<(SearchExposure, Instant)>>,
}

#[async_trait]
impl SearchExposureStore for MemorySearchExposureStore {
    async fn record(&self, exposure: SearchExposure) -> Result<(), SearchExposureError> {
        let mut exposures = self.exposures.write().await;
        let now = Instant::now();
        exposures.retain(|(_, expires_at)| *expires_at > now);
        exposures.push((exposure, now + SEARCH_EXPOSURE_TTL));
        if exposures.len() > MAX_MEMORY_SEARCH_EXPOSURES {
            let overflow = exposures.len() - MAX_MEMORY_SEARCH_EXPOSURES;
            exposures.drain(..overflow);
        }
        Ok(())
    }

    async fn validate(
        &self,
        user_id: &str,
        attributions: &[SearchAttribution],
    ) -> Result<Vec<bool>, SearchExposureError> {
        let mut exposures = self.exposures.write().await;
        let now = Instant::now();
        exposures.retain(|(_, expires_at)| *expires_at > now);
        Ok(attributions
            .iter()
            .map(|attribution| {
                exposures.iter().any(|(exposure, _)| {
                    exposure.request_id == attribution.request_id
                        && exposure.user_id == user_id
                        && exposure.session_id == attribution.session_id
                        && exposure.items.iter().any(|item| {
                            usize::try_from(attribution.position)
                                .is_ok_and(|position| position == item.position)
                                && item.result_id == attribution.result_id
                        })
                })
            })
            .collect())
    }
}

pub(crate) struct PostgresSearchExposureStore {
    pool: sqlx::PgPool,
}

impl PostgresSearchExposureStore {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchExposureStore for PostgresSearchExposureStore {
    async fn record(&self, exposure: SearchExposure) -> Result<(), SearchExposureError> {
        let mut tx = self.pool.begin().await?;
        // Retain only the short attribution window while bounding cleanup work
        // for a single search response.
        sqlx::query("WITH expired AS (DELETE FROM search_exposures WHERE request_id IN (SELECT request_id FROM search_exposures WHERE expires_at <= now() ORDER BY expires_at LIMIT $1 FOR UPDATE SKIP LOCKED)) INSERT INTO search_exposures (request_id,user_id,session_id,query_hash,query_rewrite_version,result_count,degraded,expires_at) VALUES ($2,$3,$4,$5,$6,$7,$8,now() + ($9 * interval '1 second'))")
            .bind(SEARCH_EXPOSURE_CLEANUP_BATCH_SIZE)
            .bind(&exposure.request_id)
            .bind(&exposure.user_id)
            .bind(&exposure.session_id)
            .bind(&exposure.query_hash)
            .bind(&exposure.query_rewrite_version)
            .bind(i32::try_from(exposure.items.len()).unwrap_or(i32::MAX))
            .bind(exposure.degraded)
            .bind(SEARCH_EXPOSURE_TTL.as_secs() as i64)
            .execute(&mut *tx)
            .await?;
        for item in &exposure.items {
            sqlx::query("INSERT INTO search_exposure_items (request_id,position,result_id,result_type) VALUES ($1,$2,$3,$4)")
                .bind(&exposure.request_id)
                .bind(i32::try_from(item.position).unwrap_or(i32::MAX))
                .bind(&item.result_id)
                .bind(&item.result_type)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn validate(
        &self,
        user_id: &str,
        attributions: &[SearchAttribution],
    ) -> Result<Vec<bool>, SearchExposureError> {
        if attributions.is_empty() {
            return Ok(Vec::new());
        }
        let positions = attributions
            .iter()
            .map(|item| i32::try_from(item.position))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| SearchExposureError::PositionOutOfRange)?;
        let rows = sqlx::query_as::<_, (i64, bool)>(
            "SELECT input.ordinality, EXISTS (SELECT 1 FROM search_exposures AS exposure INNER JOIN search_exposure_items AS item ON item.request_id = exposure.request_id WHERE exposure.request_id = input.request_id AND exposure.user_id = $1 AND exposure.session_id = input.session_id AND item.position = input.position AND item.result_id = input.result_id) AS valid FROM unnest($2::text[], $3::text[], $4::text[], $5::integer[]) WITH ORDINALITY AS input(request_id, session_id, result_id, position, ordinality) ORDER BY input.ordinality",
        )
        .bind(user_id)
        .bind(attributions.iter().map(|item| item.request_id.clone()).collect::<Vec<_>>())
        .bind(attributions.iter().map(|item| item.session_id.clone()).collect::<Vec<_>>())
        .bind(attributions.iter().map(|item| item.result_id.clone()).collect::<Vec<_>>())
        .bind(positions)
        .fetch_all(&self.pool)
        .await?;
        let mut valid = vec![false; attributions.len()];
        for (ordinality, is_valid) in rows {
            if let Some(index) = usize::try_from(ordinality)
                .ok()
                .and_then(|value| value.checked_sub(1))
                .filter(|index| *index < valid.len())
            {
                valid[index] = is_valid;
            }
        }
        Ok(valid)
    }
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
