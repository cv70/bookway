use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bookway_bbs_link_api::pb as bbs_link_pb;
use bookway_bbs_search_api::pb;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Error)]
pub(crate) enum SearchSourceError {
    #[error("content index source request failed: {0}")]
    Request(String),
    #[error("search snapshot expired")]
    CursorExpired,
    #[error("primary search source is unavailable")]
    Fallback,
}

pub(crate) struct SearchSourceResult {
    pub(crate) page: bbs_link_pb::ContentPage,
    pub(crate) degraded: bool,
    /// OpenSearch has already applied a stable relevance ordering for this page.
    pub(crate) source_ranked: bool,
}

/// State kept server-side while a client consumes a multi-page search result.
/// Keeping the source cursor and unconsumed mixed results here prevents large, mutable
/// OpenSearch PIT tokens from becoming public API cursors.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SearchSession {
    pub(crate) query_fingerprint: u64,
    pub(crate) source_cursor: Option<String>,
    pub(crate) source_exhausted: bool,
    pub(crate) pending: Vec<pb::SearchResult>,
    pub(crate) seen_result_ids: HashSet<String>,
    pub(crate) delivered_count: usize,
    pub(crate) source_total_estimate: usize,
    pub(crate) degraded: bool,
}

#[async_trait]
pub(crate) trait SearchSessionStore: Send + Sync {
    async fn create(&self, session: SearchSession) -> Result<String, SearchSourceError>;
    async fn load(&self, id: &str) -> Result<Option<SearchSession>, SearchSourceError>;
    /// Returns false when the session has expired between load and save.
    async fn save(&self, id: &str, session: SearchSession) -> Result<bool, SearchSourceError>;
    async fn delete(&self, id: &str) -> Result<(), SearchSourceError>;
}

const SEARCH_SESSION_TTL: Duration = Duration::from_secs(5 * 60);

/// Field bias for entity-level search surfaces (route nodes, scene gear).
/// The OpenSearch source narrows its match fields and requires route-action
/// presence; the BBS Link fallback ignores it and domain-side extraction
/// guarantees the same typed results from plain content reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntityBias {
    /// Match inside route action node titles/details/ids.
    ActionNode,
    /// Match scene equipment terms referenced by route nodes.
    SceneEquipment,
}

#[async_trait]
pub(crate) trait SearchSource: Send + Sync {
    async fn contents(
        &self,
        query: bbs_link_pb::ListRequest,
    ) -> Result<SearchSourceResult, SearchSourceError>;

    async fn search_contents(
        &self,
        query: bbs_link_pb::ListRequest,
        _text: &str,
        _excluded_author_ids: &[String],
        _entity_bias: Option<EntityBias>,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        self.contents(query).await
    }

    /// Releases a cursor that a one-shot caller intentionally will not continue.
    async fn release_search_cursor(&self, _cursor: &str) {}
}

#[async_trait]
pub(crate) trait SearchAnalytics: Send + Sync {
    async fn record(
        &self,
        user_id: Option<&str>,
        query: &str,
        search_type: pb::SearchType,
        zero_results: bool,
    );
    async fn suggestions(
        &self,
        user_id: Option<&str>,
        prefix: &str,
        limit: usize,
    ) -> Vec<pb::Suggestion>;
}

pub(crate) type SharedSearchAnalytics = Arc<dyn SearchAnalytics>;
type SearchCounters = (u64, u64);
type SearchStatsKey = (String, pb::SearchType);
type SearchHistoryKey = (String, String, pb::SearchType);

fn suggestion_score(requests: u64, zero_results: u64) -> f64 {
    ((requests.saturating_sub(zero_results)) as f64 + 1.0).ln_1p()
}

fn result_type(search_type: pb::SearchType) -> pb::SearchResultType {
    match search_type {
        pb::SearchType::Journeys => pb::SearchResultType::Journey,
        pb::SearchType::Users => pb::SearchResultType::User,
        pb::SearchType::Topics => pb::SearchResultType::Topic,
        pb::SearchType::Resources => pb::SearchResultType::Resource,
        pb::SearchType::Nodes => pb::SearchResultType::ActionNode,
        pb::SearchType::Equipment => pb::SearchResultType::SceneEquipment,
        pb::SearchType::All | pb::SearchType::Posts => pb::SearchResultType::Post,
    }
}

fn result_type_from_name(value: &str) -> pb::SearchResultType {
    match value {
        "journeys" => pb::SearchResultType::Journey,
        "users" => pb::SearchResultType::User,
        "topics" => pb::SearchResultType::Topic,
        "resources" => pb::SearchResultType::Resource,
        "nodes" => pb::SearchResultType::ActionNode,
        "equipment" => pb::SearchResultType::SceneEquipment,
        _ => pb::SearchResultType::Post,
    }
}

pub(crate) fn search_type_name(value: pb::SearchType) -> &'static str {
    match value {
        pb::SearchType::All => "all",
        pb::SearchType::Posts => "posts",
        pb::SearchType::Journeys => "journeys",
        pb::SearchType::Users => "users",
        pb::SearchType::Topics => "topics",
        pb::SearchType::Resources => "resources",
        pb::SearchType::Nodes => "nodes",
        pb::SearchType::Equipment => "equipment",
    }
}

pub(crate) fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::{MemorySearchAnalytics, SearchAnalytics};
    use bookway_bbs_search_api::pb;

    #[tokio::test]
    async fn personal_history_is_prioritized_and_never_shared_between_users() {
        let analytics = MemorySearchAnalytics::default();
        analytics
            .record(Some("user-a"), "我的晨跑调整", pb::SearchType::Posts, false)
            .await;
        analytics
            .record(
                Some("user-b"),
                "别人的晨跑调整",
                pb::SearchType::Posts,
                false,
            )
            .await;
        analytics
            .record(Some("user-a"), "我的晨跑调整", pb::SearchType::Posts, false)
            .await;

        let own = analytics.suggestions(Some("user-a"), "晨跑", 8).await;
        assert_eq!(
            own.first().map(|item| item.text.as_str()),
            Some("我的晨跑调整")
        );
        assert!(!own.iter().any(|item| item.text == "别人的晨跑调整"));

        let anonymous = analytics.suggestions(None, "晨跑", 8).await;
        assert!(!anonymous.iter().any(|item| item.text == "我的晨跑调整"));
    }
}

const PIT_KEEP_ALIVE: &str = "5m";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct PitCursor {
    id: String,
    search_after: Option<Vec<serde_json::Value>>,
    seen_hits: usize,
}

fn encode_pit_cursor(cursor: &PitCursor) -> Result<String, SearchSourceError> {
    // This cursor is stored only in the server-side search session, never returned to clients.
    serde_json::to_string(cursor)
        .map(|value| format!("pit2:{value}"))
        .map_err(|error| SearchSourceError::Request(error.to_string()))
}

fn decode_pit_cursor(value: &str) -> Result<PitCursor, SearchSourceError> {
    let value = value
        .strip_prefix("pit2:")
        .ok_or(SearchSourceError::CursorExpired)?;
    serde_json::from_str(value).map_err(|_| SearchSourceError::CursorExpired)
}

fn pit_expired(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::BAD_REQUEST
    )
}

fn content_type_name(value: i32) -> &'static str {
    match bbs_link_pb::ContentType::try_from(value) {
        Ok(bbs_link_pb::ContentType::Note) => "note",
        Ok(bbs_link_pb::ContentType::Article) => "article",
        Ok(bbs_link_pb::ContentType::Video) => "video",
        Ok(bbs_link_pb::ContentType::Route) => "route",
        Ok(bbs_link_pb::ContentType::Milestone) => "milestone",
        Ok(bbs_link_pb::ContentType::Question) => "question",
        Err(_) => "route",
    }
}

fn domain_name(value: i32) -> &'static str {
    match bbs_link_pb::GrowthDomain::try_from(value) {
        Ok(bbs_link_pb::GrowthDomain::Learning) => "learning",
        Ok(bbs_link_pb::GrowthDomain::Movement) => "movement",
        Ok(bbs_link_pb::GrowthDomain::Wellness) => "wellness",
        Ok(bbs_link_pb::GrowthDomain::Travel) => "travel",
        Ok(bbs_link_pb::GrowthDomain::Leisure) | Err(_) => "leisure",
    }
}

fn resource_url(base_url: &str, path: &[&str]) -> Result<reqwest::Url, SearchSourceError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|error| SearchSourceError::Request(error.to_string()))?;
    let mut segments = url.path_segments_mut().map_err(|_| {
        SearchSourceError::Request("OPENSEARCH_URL cannot be used as a base URL".to_string())
    })?;
    segments.pop_if_empty();
    for segment in path {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

#[path = "memory_search_session_store.rs"]
mod memory_search_session_store;
pub(crate) use memory_search_session_store::MemorySearchSessionStore;
#[path = "postgres_search_session_store.rs"]
mod postgres_search_session_store;
pub(crate) use postgres_search_session_store::PostgresSearchSessionStore;
#[path = "open_search_source.rs"]
mod open_search_source;
pub(crate) use open_search_source::OpenSearchSource;
#[path = "memory_search_analytics.rs"]
mod memory_search_analytics;
pub(crate) use memory_search_analytics::MemorySearchAnalytics;
#[path = "postgres_search_analytics.rs"]
mod postgres_search_analytics;
pub(crate) use postgres_search_analytics::PostgresSearchAnalytics;
