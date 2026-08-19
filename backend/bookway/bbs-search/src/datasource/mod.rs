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

#[derive(Default)]
pub(crate) struct MemorySearchSessionStore {
    sessions: RwLock<HashMap<String, (SearchSession, Instant)>>,
}

#[async_trait]
impl SearchSessionStore for MemorySearchSessionStore {
    async fn create(&self, session: SearchSession) -> Result<String, SearchSourceError> {
        let id = Uuid::now_v7().to_string();
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        sessions.insert(id.clone(), (session, now + SEARCH_SESSION_TTL));
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchSession>, SearchSourceError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        Ok(sessions.get(id).map(|(session, _)| session.clone()))
    }

    async fn save(&self, id: &str, session: SearchSession) -> Result<bool, SearchSourceError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        let Some((stored, expires_at)) = sessions.get_mut(id) else {
            return Ok(false);
        };
        *stored = session;
        *expires_at = now + SEARCH_SESSION_TTL;
        Ok(true)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSourceError> {
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
    async fn create(&self, session: SearchSession) -> Result<String, SearchSourceError> {
        let id = Uuid::now_v7().to_string();
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        sqlx::query(
            "WITH expired AS (DELETE FROM search_sessions WHERE expires_at <= now()) INSERT INTO search_sessions (session_id,state,expires_at) VALUES ($1,$2,now() + ($3 * interval '1 second'))",
        )
        .bind(&id)
        .bind(state)
        .bind(SEARCH_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchSession>, SearchSourceError> {
        let state = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT state FROM search_sessions WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        state
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| SearchSourceError::Request(error.to_string()))
    }

    async fn save(&self, id: &str, session: SearchSession) -> Result<bool, SearchSourceError> {
        let state = serde_json::to_value(session)
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE search_sessions SET state = $2, expires_at = now() + ($3 * interval '1 second') WHERE session_id = $1 AND expires_at > now()",
        )
        .bind(id)
        .bind(state)
        .bind(SEARCH_SESSION_TTL.as_secs() as i64)
        .execute(&self.pool)
        .await
        .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSourceError> {
        sqlx::query("DELETE FROM search_sessions WHERE session_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        Ok(())
    }
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
    ) -> Result<SearchSourceResult, SearchSourceError> {
        self.contents(query).await
    }

    /// Releases a cursor that a one-shot caller intentionally will not continue.
    async fn release_search_cursor(&self, _cursor: &str) {}
}

pub(crate) struct OpenSearchSource {
    client: reqwest::Client,
    base_url: String,
    read_alias: String,
}

impl OpenSearchSource {
    pub(crate) fn new(base_url: String, read_alias: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            read_alias,
        }
    }
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

#[derive(Default)]
pub(crate) struct MemorySearchAnalytics {
    stats: RwLock<HashMap<(String, pb::SearchType), (u64, u64)>>,
    global_users: RwLock<HashMap<(String, pb::SearchType), HashSet<String>>>,
    history: RwLock<HashMap<(String, String, pb::SearchType), (u64, u64)>>,
    sequence: RwLock<u64>,
}

#[async_trait]
impl SearchAnalytics for MemorySearchAnalytics {
    async fn record(
        &self,
        user_id: Option<&str>,
        query: &str,
        search_type: pb::SearchType,
        zero_results: bool,
    ) {
        let mut stats = self.stats.write().await;
        let value = stats
            .entry((query.to_string(), search_type))
            .or_insert((0, 0));
        value.0 = value.0.saturating_add(1);
        value.1 = value.1.saturating_add(u64::from(zero_results));
        let user_id = user_id.map(str::trim).filter(|id| !id.is_empty());
        if let Some(user_id) = user_id {
            self.global_users
                .write()
                .await
                .entry((query.to_string(), search_type))
                .or_default()
                .insert(user_id.to_string());
        }
        let Some(user_id) = user_id else { return };
        let mut sequence = self.sequence.write().await;
        *sequence = sequence.saturating_add(1);
        self.history.write().await.insert(
            (user_id.to_string(), query.to_string(), search_type),
            (value.0, *sequence),
        );
    }

    async fn suggestions(
        &self,
        user_id: Option<&str>,
        prefix: &str,
        limit: usize,
    ) -> Vec<pb::Suggestion> {
        let prefix = prefix.to_lowercase();
        let user_id = user_id.map(str::trim).filter(|id| !id.is_empty());
        let mut personal = {
            let history = self.history.read().await;
            history
                .iter()
                .filter(|((owner, query, _), _)| {
                    Some(owner.as_str()) == user_id && query.to_lowercase().contains(&prefix)
                })
                .map(
                    |((_, query, search_type), (requests, sequence))| pb::Suggestion {
                        text: query.clone(),
                        result_type: result_type(*search_type) as i32,
                        score: 10.0
                            + (*sequence as f64 / 1_000_000_000.0)
                            + suggestion_score(*requests, 0),
                        personal: true,
                    },
                )
                .collect::<Vec<_>>()
        };
        personal.sort_by(|left, right| right.score.total_cmp(&left.score));
        personal.truncate(limit);
        let stats = self.stats.read().await.clone();
        let global_users = self.global_users.read().await;
        let mut items = stats
            .iter()
            .filter(|((query, search_type), (requests, _))| {
                let unique_users = global_users
                    .get(&(query.clone(), *search_type))
                    .map_or(0, HashSet::len);
                *requests >= 2
                    && (unique_users == 0 || unique_users >= 2)
                    && query.to_lowercase().contains(&prefix)
            })
            .map(
                |((query, search_type), (requests, zero_results))| pb::Suggestion {
                    text: query.clone(),
                    result_type: result_type(*search_type) as i32,
                    score: suggestion_score(*requests, *zero_results),
                    personal: false,
                },
            )
            .collect::<Vec<_>>();
        items.retain(|item| !personal.iter().any(|owned| owned.text == item.text));
        personal.extend(items);
        let mut items = personal;
        items.sort_by(|left, right| right.score.total_cmp(&left.score));
        items.truncate(limit);
        items
    }
}

pub(crate) struct PostgresSearchAnalytics {
    pool: sqlx::PgPool,
}

impl PostgresSearchAnalytics {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchAnalytics for PostgresSearchAnalytics {
    async fn record(
        &self,
        user_id: Option<&str>,
        query: &str,
        search_type: pb::SearchType,
        zero_results: bool,
    ) {
        let search_type = search_type_name(search_type);
        let hash = format!("{:016x}", stable_hash(&format!("{search_type}\0{query}")));
        if let Err(error) = sqlx::query(
            "INSERT INTO search_query_stats (query_hash,query_text,search_type,request_count,zero_result_count,last_seen_at) VALUES ($1,$2,$3,1,$4,now()) ON CONFLICT (query_hash) DO UPDATE SET request_count=search_query_stats.request_count+1, zero_result_count=search_query_stats.zero_result_count+EXCLUDED.zero_result_count, last_seen_at=now()",
        )
        .bind(&hash)
        .bind(query)
        .bind(search_type)
        .bind(i64::from(zero_results))
        .execute(&self.pool)
        .await
        {
            tracing::warn!(%error, "search analytics write degraded");
        }
        if let Some(user_id) = user_id.map(str::trim).filter(|id| !id.is_empty()) {
            let history_hash = format!("{:016x}", stable_hash(&format!("{user_id}\0{query}")));
            if let Err(error) = sqlx::query(
                "INSERT INTO search_query_history (history_hash,user_id,query_text,search_type,request_count,last_seen_at) VALUES ($1,$2,$3,$4,1,now()) ON CONFLICT (history_hash) DO UPDATE SET request_count=search_query_history.request_count+1,last_seen_at=now()",
            )
            .bind(history_hash)
            .bind(user_id)
            .bind(query)
            .bind(search_type)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "personal search history write degraded");
            }
            if let Err(error) = sqlx::query(
                "INSERT INTO search_query_stats_users (query_hash,user_id,last_seen_at) VALUES ($1,$2,now()) ON CONFLICT (query_hash,user_id) DO UPDATE SET last_seen_at=now()",
            )
            .bind(&hash)
            .bind(user_id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "search query unique-user write degraded");
            }
            if let Err(error) = sqlx::query(
                "DELETE FROM search_query_history WHERE user_id = $1 AND last_seen_at < now() - interval '90 days'",
            )
            .bind(user_id)
            .execute(&self.pool)
            .await
            {
                tracing::warn!(%error, "personal search history retention cleanup degraded");
            }
        }
    }

    async fn suggestions(
        &self,
        user_id: Option<&str>,
        prefix: &str,
        limit: usize,
    ) -> Vec<pb::Suggestion> {
        let pattern = format!("%{}%", escape_like(prefix));
        let mut items = Vec::new();
        if let Some(user_id) = user_id.map(str::trim).filter(|id| !id.is_empty()) {
            let rows = sqlx::query_as::<_, (String, String, i64)>(&format!(
                "SELECT query_text,search_type,request_count FROM search_query_history WHERE user_id = $1 AND query_text ILIKE $2 ESCAPE '\\\\' AND last_seen_at > now() - interval '90 days' ORDER BY last_seen_at DESC LIMIT {}",
                limit
            ))
            .bind(user_id)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await;
            if let Ok(rows) = rows {
                items.extend(rows.into_iter().map(|(text, search_type, requests)| {
                    pb::Suggestion {
                        text,
                        result_type: result_type_from_name(&search_type) as i32,
                        score: 10.0 + suggestion_score(requests.max(0) as u64, 0),
                        personal: true,
                    }
                }));
            }
        }
        let rows = sqlx::query_as::<_, (String, String, i64, i64)>(
            "SELECT stats.query_text,stats.search_type,stats.request_count,stats.zero_result_count FROM search_query_stats AS stats LEFT JOIN (SELECT query_hash,COUNT(*) AS unique_users FROM search_query_stats_users GROUP BY query_hash) AS users ON users.query_hash = stats.query_hash WHERE stats.request_count >= 2 AND (COALESCE(users.unique_users, 0) = 0 OR users.unique_users >= 2) AND stats.query_text ILIKE $1 ESCAPE '\\' AND stats.last_seen_at > now() - interval '90 days' ORDER BY (stats.request_count-stats.zero_result_count) DESC,stats.last_seen_at DESC LIMIT $2",
        )
        .bind(pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(rows) => {
                items.extend(rows.into_iter().map(
                    |(text, search_type, requests, zero_results)| pb::Suggestion {
                        text,
                        result_type: result_type_from_name(&search_type) as i32,
                        score: suggestion_score(requests.max(0) as u64, zero_results.max(0) as u64),
                        personal: false,
                    },
                ));
                items.sort_by(|left, right| right.score.total_cmp(&left.score));
                items.dedup_by(|left, right| left.text == right.text);
                items.truncate(limit);
                items
            }
            Err(error) => {
                tracing::warn!(%error, "search analytics suggestions degraded");
                items.truncate(limit);
                items
            }
        }
    }
}

fn suggestion_score(requests: u64, zero_results: u64) -> f64 {
    ((requests.saturating_sub(zero_results)) as f64 + 1.0).ln_1p()
}

fn result_type(search_type: pb::SearchType) -> pb::SearchResultType {
    match search_type {
        pb::SearchType::Journeys => pb::SearchResultType::Journey,
        pb::SearchType::Users => pb::SearchResultType::User,
        pb::SearchType::Topics => pb::SearchResultType::Topic,
        pb::SearchType::Resources => pb::SearchResultType::Resource,
        pb::SearchType::All | pb::SearchType::Posts => pb::SearchResultType::Post,
    }
}

fn result_type_from_name(value: &str) -> pb::SearchResultType {
    match value {
        "journeys" => pb::SearchResultType::Journey,
        "users" => pb::SearchResultType::User,
        "topics" => pb::SearchResultType::Topic,
        "resources" => pb::SearchResultType::Resource,
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

#[async_trait]
impl SearchSource for OpenSearchSource {
    async fn contents(
        &self,
        _query: bbs_link_pb::ListRequest,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        Err(SearchSourceError::Fallback)
    }

    async fn search_contents(
        &self,
        query: bbs_link_pb::ListRequest,
        text: &str,
        excluded_author_ids: &[String],
    ) -> Result<SearchSourceResult, SearchSourceError> {
        if query
            .cursor
            .as_deref()
            .is_some_and(|cursor| cursor.starts_with("fallback:"))
        {
            return Err(SearchSourceError::Fallback);
        }
        let pit_cursor = match query.cursor.as_deref() {
            Some(cursor) => decode_pit_cursor(cursor)?,
            None => match self.open_pit().await {
                Ok(id) => PitCursor {
                    id,
                    search_after: None,
                    seen_hits: 0,
                },
                Err(_) => return Err(SearchSourceError::Fallback),
            },
        };
        let mut filters = vec![serde_json::json!({ "term": { "status": "published" } })];
        if let Some(content_type) = query.content_type {
            filters.push(
                serde_json::json!({ "term": { "content_type": content_type_name(content_type) } }),
            );
        }
        if let Some(domain) = query.domain {
            filters.push(serde_json::json!({ "term": { "domain": domain_name(domain) } }));
        }
        let mut body = serde_json::json!({
            "size": query.limit.unwrap_or(100).clamp(1, 100),
            "track_total_hits": true,
            "pit": { "id": pit_cursor.id, "keep_alive": PIT_KEEP_ALIVE },
            "sort": [{ "_score": "desc" }, { "id.keyword": "asc" }],
            "query": { "bool": { "must": [{ "multi_match": { "query": text, "fields": ["title^4", "summary^2", "route_action_titles^3", "route_scene_equipment^3", "route_action_details^2", "route_action_ids", "body", "tags", "topics", "author_name"], "type": "best_fields" }}], "filter": filters }},
            "highlight": { "fields": { "title": {}, "summary": {}, "body": {} } }
        });
        if !excluded_author_ids.is_empty() {
            body["query"]["bool"]["must_not"] = serde_json::json!([
                { "terms": { "author_id": excluded_author_ids } }
            ]);
        }
        if let Some(search_after) = pit_cursor.search_after.clone()
            && let Some(object) = body.as_object_mut()
        {
            object.insert(
                "search_after".to_string(),
                serde_json::Value::Array(search_after),
            );
        }
        let response = self
            .client
            .post(format!("{}/_search", self.base_url))
            .json(&body)
            .send()
            .await;
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if pit_expired(response.status()) && query.cursor.is_some() => {
                return Err(SearchSourceError::CursorExpired);
            }
            Ok(response) if pit_expired(response.status()) => {
                self.close_pit(&pit_cursor.id).await;
                return Err(SearchSourceError::Fallback);
            }
            Ok(response) => {
                // A new query can safely fall back. A continuation must retain its snapshot
                // boundary, so its caller receives an explicit expiry instead of mixed order.
                if query.cursor.is_some() {
                    return Err(SearchSourceError::Request(format!(
                        "OpenSearch search returned {}",
                        response.status()
                    )));
                }
                self.close_pit(&pit_cursor.id).await;
                return Err(SearchSourceError::Fallback);
            }
            Err(error) if query.cursor.is_some() => {
                return Err(SearchSourceError::Request(error.to_string()));
            }
            Err(_) => {
                self.close_pit(&pit_cursor.id).await;
                return Err(SearchSourceError::Fallback);
            }
        };
        let payload: serde_json::Value = match response.json().await {
            Ok(payload) => payload,
            Err(_) if query.cursor.is_none() => {
                self.close_pit(&pit_cursor.id).await;
                return Err(SearchSourceError::Fallback);
            }
            Err(error) => return Err(SearchSourceError::Request(error.to_string())),
        };
        let hits = payload
            .get("hits")
            .and_then(|value| value.get("hits"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .ok_or_else(|| SearchSourceError::Request("OpenSearch hits missing".to_string()))?;
        let hit_count = hits.len();
        let last_sort = hits
            .last()
            .and_then(|hit| hit.get("sort"))
            .and_then(serde_json::Value::as_array)
            .cloned();
        let items = hits
            .iter()
            .map(|hit| {
                hit.get("_source")
                    .cloned()
                    .ok_or_else(|| {
                        SearchSourceError::Request("OpenSearch hit source missing".to_string())
                    })
                    .and_then(|source| {
                        serde_json::from_value(source)
                            .map_err(|error| SearchSourceError::Request(error.to_string()))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let total = payload
            .get("hits")
            .and_then(|value| value.get("total"))
            .and_then(|value| value.get("value"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(items.len() as u64) as usize;
        let active_pit_id = payload
            .get("pit_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&pit_cursor.id)
            .to_string();
        let next_cursor = if pit_cursor.seen_hits + hit_count < total {
            let Some(search_after) = last_sort else {
                self.close_pit(&active_pit_id).await;
                return Err(SearchSourceError::Request(
                    "OpenSearch hit sort values missing".to_string(),
                ));
            };
            Some(encode_pit_cursor(&PitCursor {
                id: active_pit_id.clone(),
                search_after: Some(search_after),
                seen_hits: pit_cursor.seen_hits + hit_count,
            })?)
        } else {
            None
        };
        if next_cursor.is_none() {
            self.close_pit(&active_pit_id).await;
        }
        Ok(SearchSourceResult {
            page: bbs_link_pb::ContentPage {
                next_cursor,
                total_estimate: total as u64,
                items,
            },
            degraded: false,
            source_ranked: true,
        })
    }

    async fn release_search_cursor(&self, cursor: &str) {
        if let Ok(cursor) = decode_pit_cursor(cursor) {
            self.close_pit(&cursor.id).await;
        }
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

impl OpenSearchSource {
    async fn open_pit(&self) -> Result<String, SearchSourceError> {
        let mut pit_url = resource_url(
            &self.base_url,
            &[&self.read_alias, "_search", "point_in_time"],
        )?;
        pit_url
            .query_pairs_mut()
            .append_pair("keep_alive", PIT_KEEP_ALIVE);
        let response = self
            .client
            .post(pit_url)
            .send()
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        if !response.status().is_success() {
            return Err(SearchSourceError::Request(format!(
                "OpenSearch PIT creation returned {}",
                response.status()
            )));
        }
        let payload = response
            .json::<serde_json::Value>()
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        payload
            .get("pit_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .ok_or_else(|| SearchSourceError::Request("OpenSearch PIT id missing".to_string()))
    }

    async fn close_pit(&self, id: &str) {
        if let Err(error) = self
            .client
            .delete(format!("{}/_search/point_in_time", self.base_url))
            .json(&serde_json::json!({ "pit_id": id }))
            .send()
            .await
        {
            tracing::debug!(%error, "OpenSearch PIT close degraded");
        }
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
