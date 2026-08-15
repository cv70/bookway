use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bookway_api::SearchResultDto;
use bookway_bbs_search::api::pb::{self, bbs_search_client::BbsSearchClient};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::api::{
    SearchQueryRequest, SearchResponseDto, SuggestionQueryRequest, SuggestionResponseDto,
};

#[derive(Debug, Error)]
pub(crate) enum SearchClientError {
    #[error("bbs-search request failed: {0}")]
    Transport(String),
    #[error("bbs-search request failed with {code:?}: {message}")]
    Upstream { code: tonic::Code, message: String },
}

#[derive(Debug, Error)]
pub(crate) enum SearchSessionError {
    #[error("search pipeline session storage failed: {0}")]
    Storage(String),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RecallState {
    pub(crate) query: String,
    pub(crate) source_cursor: Option<String>,
    pub(crate) exhausted: bool,
}

/// The main-search session mixes independently paged recalls without exposing
/// upstream cursor tokens to clients. Source cursors remain owned by bbs-search.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SearchPipelineSession {
    pub(crate) query_fingerprint: u64,
    pub(crate) recalls: Vec<RecallState>,
    pub(crate) pending: Vec<SearchResultDto>,
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

#[async_trait]
pub(crate) trait SearchDataSource: Send + Sync {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError>;
    async fn suggestions(
        &self,
        request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, SearchClientError>;
}

pub(crate) struct GrpcSearchDataSource {
    client: BbsSearchClient<tonic::transport::Channel>,
}

impl GrpcSearchDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: BbsSearchClient::connect(base_url).await?,
        })
    }
}

#[async_trait]
impl SearchDataSource for GrpcSearchDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchClientError> {
        let mut client = self.client.clone();
        let response = client
            .search(pb::SearchRequest {
                request_json: serde_json::to_string(&request)
                    .map_err(|error| SearchClientError::Transport(error.to_string()))?,
            })
            .await
            .map_err(|error| SearchClientError::Upstream {
                code: error.code(),
                message: error.message().to_string(),
            })?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| SearchClientError::Transport(error.to_string()))
    }

    async fn suggestions(
        &self,
        request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, SearchClientError> {
        let mut client = self.client.clone();
        let response = client
            .suggestions(pb::SuggestionsRequest {
                // Empty legacy text fails closed on an old bbs-search instance;
                // a new instance reads the policy-bearing JSON request.
                query: String::new(),
                request_json: serde_json::to_string(&request)
                    .map_err(|error| SearchClientError::Transport(error.to_string()))?,
            })
            .await
            .map_err(|error| SearchClientError::Upstream {
                code: error.code(),
                message: error.message().to_string(),
            })?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| SearchClientError::Transport(error.to_string()))
    }
}
