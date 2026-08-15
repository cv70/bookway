use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use bookway_bbs_link_api::pb::{self as bbs_link_pb, bbs_link_client::BbsLinkClient};
use bookway_bbs_search_api::pb::{self, bbs_search_client::BbsSearchClient};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    api::pb as api_pb,
    conf::Config,
    datasource::{
        MemoryQueryRewriteRepository, MemorySearchExposureStore, MemorySearchSessionStore,
        PostgresQueryRewriteRepository, PostgresSearchExposureStore, PostgresSearchSessionStore,
        QueryRewriteDictionary, RecallState, SearchAttribution, SearchExposure,
        SearchExposureError, SearchExposureItem, SearchPipelineSession, SearchSessionError,
        SearchSessionStore, SharedQueryRewriteRepository, SharedSearchExposureStore,
        builtin_query_rewrite_dictionary,
    },
};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;
const RECALL_PAGE_SIZE: usize = 50;
const MAX_RECALL_PAGES_PER_RESPONSE: usize = 8;
const MAX_PUBLIC_CURSOR_BYTES: usize = 128;
const MAX_QUERY_LENGTH: usize = 100;
const MAX_REWRITE_TERMS: usize = 6;
const QUERY_REWRITE_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const BBS_SEARCH_TIMEOUT: Duration = Duration::from_millis(1_500);
const BBS_LINK_TIMEOUT: Duration = Duration::from_millis(1_500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchIntent {
    Generic,
    Topic,
    User,
    Journey,
}

#[derive(Clone, Debug)]
struct RecallPlan {
    query: String,
}

#[derive(Clone, Debug)]
struct SearchPlan {
    original_query: String,
    recalls: Vec<RecallPlan>,
    intent: SearchIntent,
    query_rewrite_version: String,
}

#[derive(Clone)]
struct QueryRewriteResolution {
    dictionary: QueryRewriteDictionary,
    degraded: bool,
}

struct QueryRewriteCacheState {
    dictionary: QueryRewriteDictionary,
    refreshed_at: Instant,
    degraded: bool,
}

struct QueryRewriteCache {
    repository: SharedQueryRewriteRepository,
    state: RwLock<Option<QueryRewriteCacheState>>,
}

impl QueryRewriteCache {
    fn new(repository: SharedQueryRewriteRepository) -> Self {
        Self {
            repository,
            state: RwLock::new(None),
        }
    }

    async fn active(&self) -> QueryRewriteResolution {
        let stale = {
            let state = self.state.read().await;
            match state.as_ref() {
                Some(state) if state.refreshed_at.elapsed() < QUERY_REWRITE_REFRESH_INTERVAL => {
                    return QueryRewriteResolution {
                        dictionary: state.dictionary.clone(),
                        degraded: state.degraded,
                    };
                }
                Some(state) => Some(state.dictionary.clone()),
                None => None,
            }
        };
        match self.repository.active().await {
            Ok(Some(dictionary)) => match sanitize_dictionary(dictionary) {
                Some(dictionary) => {
                    self.state.write().await.replace(QueryRewriteCacheState {
                        dictionary: dictionary.clone(),
                        refreshed_at: Instant::now(),
                        degraded: false,
                    });
                    QueryRewriteResolution {
                        dictionary,
                        degraded: false,
                    }
                }
                None => {
                    self.fallback(stale, "active query rewrite dictionary is invalid")
                        .await
                }
            },
            Ok(None) => {
                self.fallback(stale, "active query rewrite dictionary is missing")
                    .await
            }
            Err(error) => self.fallback(stale, &error.to_string()).await,
        }
    }

    async fn fallback(
        &self,
        stale: Option<QueryRewriteDictionary>,
        reason: &str,
    ) -> QueryRewriteResolution {
        tracing::warn!(
            reason,
            "query rewrite configuration degraded; retaining a safe dictionary"
        );
        let dictionary = stale.unwrap_or_else(builtin_query_rewrite_dictionary);
        self.state.write().await.replace(QueryRewriteCacheState {
            dictionary: dictionary.clone(),
            refreshed_at: Instant::now(),
            degraded: true,
        });
        QueryRewriteResolution {
            dictionary,
            degraded: true,
        }
    }
}

enum PublicCursor {
    New(String),
    Legacy(String),
}

#[derive(Debug, Error)]
pub(crate) enum SearchMainError {
    #[error("search query must not be empty")]
    EmptyQuery,
    #[error("search query exceeds {MAX_QUERY_LENGTH} characters")]
    QueryTooLong,
    #[error("{0}")]
    InvalidCursor(String),
    #[error("搜索会话已过期，请重新搜索")]
    CursorExpired,
    #[error(transparent)]
    Session(#[from] SearchSessionError),
    #[error("bbs-search request failed with {code:?}: {message}")]
    Upstream { code: tonic::Code, message: String },
    #[error("bbs-link public content request failed with {code:?}: {message}")]
    ContentUpstream { code: tonic::Code, message: String },
    #[error("bbs-link returned an invalid public summary batch")]
    InvalidContentSummary,
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    search_client: BbsSearchClient<tonic::transport::Channel>,
    content_client: Option<BbsLinkClient<tonic::transport::Channel>>,
    sessions: Arc<dyn SearchSessionStore>,
    exposures: SharedSearchExposureStore,
    query_rewrites: Arc<QueryRewriteCache>,
}

impl Domain {
    pub(crate) async fn new(
        config: Config,
        search_client: BbsSearchClient<tonic::transport::Channel>,
        content_client: BbsLinkClient<tonic::transport::Channel>,
    ) -> Result<Self, bookway_data::DataError> {
        let storage = bookway_data::storage_mode()?;
        let pool = match storage {
            bookway_data::StorageMode::Memory => None,
            bookway_data::StorageMode::Postgres => Some(bookway_data::postgres_pool().await?),
        };
        let sessions: Arc<dyn SearchSessionStore> = match &pool {
            Some(pool) => Arc::new(PostgresSearchSessionStore::new(pool.clone())),
            None => Arc::new(MemorySearchSessionStore::default()),
        };
        let exposures: SharedSearchExposureStore = match &pool {
            Some(pool) => Arc::new(PostgresSearchExposureStore::new(pool.clone())),
            None => Arc::new(MemorySearchExposureStore::default()),
        };
        let query_rewrites: SharedQueryRewriteRepository = match pool {
            Some(pool) => Arc::new(PostgresQueryRewriteRepository::new(pool)),
            None => Arc::new(MemoryQueryRewriteRepository),
        };
        Ok(Self {
            config,
            search_client,
            content_client: Some(content_client),
            sessions,
            exposures,
            query_rewrites: Arc::new(QueryRewriteCache::new(query_rewrites)),
        })
    }

    #[cfg(test)]
    fn with_test_dependencies(
        search_client: BbsSearchClient<tonic::transport::Channel>,
        sessions: Arc<dyn SearchSessionStore>,
        exposures: SharedSearchExposureStore,
    ) -> Self {
        Self {
            config: Config {
                listen_addr: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
                bbs_search_url: String::new(),
                bbs_link_url: String::new(),
            },
            search_client,
            // Unit tests supply already-authoritative candidates directly. Production
            // construction above always installs the BBS Link public-fact client.
            content_client: None,
            sessions,
            exposures,
            query_rewrites: Arc::new(QueryRewriteCache::new(Arc::new(
                MemoryQueryRewriteRepository,
            ))),
        }
    }

    pub(crate) async fn search(
        &self,
        mut request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, SearchMainError> {
        let search_type = pb::SearchType::try_from(request.search_type)
            .map_err(|_| SearchMainError::InvalidCursor("搜索类型无效".to_string()))?;
        let started = Instant::now();
        let rewrite_resolution = self.query_rewrites.active().await;
        let plan = make_search_plan(&request.q, &rewrite_resolution.dictionary)?;
        request.q = plan.original_query.clone();
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        request.limit = Some(limit as u32);
        request.excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let fingerprint = query_fingerprint(
            &plan.original_query,
            search_type,
            request.user_id.as_deref(),
            &request.excluded_author_ids,
        );
        let cursor = parse_cursor(
            request.cursor.as_deref(),
            fingerprint,
            &plan.original_query,
            search_type,
            request.user_id.as_deref(),
            &request.excluded_author_ids,
        )?;
        let session_id = cursor.as_ref().and_then(|cursor| match cursor {
            PublicCursor::New(id) => Some(id.clone()),
            PublicCursor::Legacy(_) => None,
        });
        let mut session = match cursor {
            Some(PublicCursor::New(id)) => self
                .sessions
                .load(&id)
                .await?
                .filter(|session| session.query_fingerprint == fingerprint)
                .ok_or(SearchMainError::CursorExpired)?,
            Some(PublicCursor::Legacy(source_cursor)) => {
                legacy_session(fingerprint, plan.original_query.clone(), source_cursor)
            }
            None => new_session(fingerprint, &plan),
        };
        session.degraded |= rewrite_resolution.degraded;

        let mut page = Vec::with_capacity(limit);
        let mut source_calls = 0;
        while page.len() < limit {
            if !session.pending.is_empty() {
                let take = (limit - page.len()).min(session.pending.len());
                let candidates = session.pending.drain(..take).collect::<Vec<_>>();
                page.extend(self.revalidate_pending(candidates).await?);
                continue;
            }
            if all_recalls_exhausted(&session) || source_calls >= MAX_RECALL_PAGES_PER_RESPONSE {
                break;
            }
            source_calls += self
                .fetch_recall_round(&request, &plan, &mut session, source_calls)
                .await?;
        }

        session.delivered_count += page.len();
        let has_next_page = !session.pending.is_empty() || !all_recalls_exhausted(&session);
        let total_estimate = if all_recalls_exhausted(&session) {
            session.delivered_count + session.pending.len()
        } else {
            session
                .source_total_estimate
                .max(session.delivered_count + session.pending.len())
        };
        let next_cursor = if has_next_page {
            let id = match session_id.as_deref() {
                Some(id) => {
                    if !self.sessions.save(id, session.clone()).await? {
                        return Err(SearchMainError::CursorExpired);
                    }
                    id.to_string()
                }
                None => self.sessions.create(session.clone()).await?,
            };
            Some(make_cursor(fingerprint, &id))
        } else {
            if let Some(id) = session_id.as_deref() {
                self.sessions.delete(id).await?;
            }
            None
        };
        let request_id = Uuid::now_v7().to_string();
        let user_id = request
            .user_id
            .clone()
            .unwrap_or_else(|| "anonymous".to_string());
        let tracking_session_id = request
            .session_id
            .clone()
            .filter(|session_id| !session_id.trim().is_empty())
            .unwrap_or_else(|| "anonymous-search-session".to_string());
        let exposure = SearchExposure {
            request_id: request_id.clone(),
            user_id,
            session_id: tracking_session_id,
            query_hash: format!("{:016x}", stable_hash(&plan.original_query)),
            query_rewrite_version: session.query_rewrite_version.clone(),
            degraded: session.degraded,
            items: page
                .iter()
                .enumerate()
                .map(|(position, result)| SearchExposureItem {
                    position,
                    result_id: result.id.clone(),
                    result_type: pb::SearchResultType::try_from(result.result_type)
                        .map_or("unknown", |result_type| result_type.as_str_name())
                        .to_string(),
                })
                .collect(),
        };
        let exposure_degraded = match self.exposures.record(exposure).await {
            Ok(()) => false,
            Err(error) => {
                tracing::warn!(%error, request_id = %request_id, "search exposure persistence degraded");
                true
            }
        };
        tracing::debug!(
            query_hash = format_args!("{:016x}", stable_hash(&plan.original_query)),
            variants = session.recalls.len(),
            source_calls,
            candidates = session.delivered_count + session.pending.len(),
            took_ms = started.elapsed().as_millis() as u64,
            degraded = session.degraded || exposure_degraded,
            "search pipeline completed"
        );
        Ok(pb::SearchResponse {
            query: plan.original_query,
            items: page,
            next_cursor,
            total_estimate: u64::try_from(total_estimate).unwrap_or(u64::MAX),
            took_ms: started.elapsed().as_millis() as u64,
            degraded: session.degraded || exposure_degraded,
            request_id,
        })
    }

    pub(crate) async fn validate_attributions(
        &self,
        request: api_pb::ValidateSearchAttributionsRequest,
    ) -> Result<api_pb::ValidateSearchAttributionsResponse, SearchExposureError> {
        if request
            .attributions
            .iter()
            .any(|attribution| attribution.position > i32::MAX as u32)
        {
            return Err(SearchExposureError::PositionOutOfRange);
        }
        let attributions = request
            .attributions
            .into_iter()
            .map(|attribution| SearchAttribution {
                request_id: attribution.request_id,
                session_id: attribution.session_id,
                result_id: attribution.result_id,
                position: attribution.position,
            })
            .collect::<Vec<_>>();
        let valid = self
            .exposures
            .validate(&request.user_id, &attributions)
            .await?;
        Ok(api_pb::ValidateSearchAttributionsResponse { valid })
    }

    async fn fetch_recall_round(
        &self,
        request: &pb::SearchRequest,
        plan: &SearchPlan,
        session: &mut SearchPipelineSession,
        source_calls: usize,
    ) -> Result<usize, SearchMainError> {
        let mut calls = 0;
        for index in 0..session.recalls.len() {
            if source_calls + calls >= MAX_RECALL_PAGES_PER_RESPONSE {
                break;
            }
            let recall = &session.recalls[index];
            if recall.exhausted {
                continue;
            }
            let mut source_request = request.clone();
            source_request.q = recall.query.clone();
            source_request.cursor = recall.source_cursor.clone();
            source_request.limit = Some(RECALL_PAGE_SIZE as u32);
            calls += 1;

            match self.search_bbs(source_request).await {
                Ok(response) => {
                    let recall = &mut session.recalls[index];
                    recall.source_cursor = response.next_cursor;
                    recall.exhausted = recall.source_cursor.is_none();
                    session.source_total_estimate = session
                        .source_total_estimate
                        .max(usize::try_from(response.total_estimate).unwrap_or(usize::MAX));
                    session.degraded |= response.degraded;
                    let mut candidates = response.items;
                    rerank_results(&mut candidates, &plan.original_query, plan.intent);
                    merge_candidates(
                        &mut session.pending,
                        &mut session.seen_result_ids,
                        candidates,
                    );
                }
                Err(error) if index == 0 => return Err(error),
                Err(error) => {
                    // An expansion is optional; keep exact lexical results available.
                    session.recalls[index].exhausted = true;
                    session.degraded = true;
                    tracing::warn!(
                        variant = index,
                        error = %error,
                        "search expansion recall degraded"
                    );
                }
            }
        }
        sort_results(&mut session.pending);
        Ok(calls)
    }

    async fn revalidate_pending(
        &self,
        candidates: Vec<pb::SearchResult>,
    ) -> Result<Vec<pb::SearchResult>, SearchMainError> {
        let Some(content_client) = self.content_client.as_ref() else {
            return Ok(candidates);
        };
        let ids = pending_content_ids(&candidates)?;
        if ids.is_empty() {
            return Ok(candidates);
        }
        let mut client = content_client.clone();
        let summaries = tokio::time::timeout(
            BBS_LINK_TIMEOUT,
            client.get_public_summaries(
                bookway_runtime::grpc_service_request(bbs_link_pb::PublicContentSummariesRequest {
                    ids: ids.into_iter().collect(),
                })
                .map_err(|error| SearchMainError::ContentUpstream {
                    code: tonic::Code::Internal,
                    message: error.to_string(),
                })?,
            ),
        )
        .await
        .map_err(|_| SearchMainError::ContentUpstream {
            code: tonic::Code::DeadlineExceeded,
            message: "bbs-link public content request timed out".to_string(),
        })?
        .map_err(|error| SearchMainError::ContentUpstream {
            code: error.code(),
            message: error.message().to_string(),
        })?
        .into_inner();
        reconcile_pending_results(candidates, summaries)
    }

    pub(crate) async fn suggestions(
        &self,
        mut request: pb::SuggestionsRequest,
    ) -> Result<pb::SuggestionsResponse, SearchMainError> {
        request.q = normalize_query(&request.q)?;
        request.excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        self.suggest_bbs(request).await
    }

    async fn search_bbs(
        &self,
        request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, SearchMainError> {
        let mut client = self.search_client.clone();
        let response = tokio::time::timeout(BBS_SEARCH_TIMEOUT, client.search(request))
            .await
            .map_err(|_| SearchMainError::Upstream {
                code: tonic::Code::DeadlineExceeded,
                message: "bbs-search search request timed out".to_string(),
            })?
            .map_err(|error| SearchMainError::Upstream {
                code: error.code(),
                message: error.message().to_string(),
            })?
            .into_inner();
        Ok(response)
    }

    async fn suggest_bbs(
        &self,
        request: pb::SuggestionsRequest,
    ) -> Result<pb::SuggestionsResponse, SearchMainError> {
        let mut client = self.search_client.clone();
        let response = tokio::time::timeout(BBS_SEARCH_TIMEOUT, client.suggestions(request))
            .await
            .map_err(|_| SearchMainError::Upstream {
                code: tonic::Code::DeadlineExceeded,
                message: "bbs-search suggestions request timed out".to_string(),
            })?
            .map_err(|error| SearchMainError::Upstream {
                code: error.code(),
                message: error.message().to_string(),
            })?
            .into_inner();
        Ok(response)
    }
}

fn pending_content_ids(
    candidates: &[pb::SearchResult],
) -> Result<BTreeSet<String>, SearchMainError> {
    let mut ids = BTreeSet::new();
    for candidate in candidates {
        if !matches!(
            pb::SearchResultType::try_from(candidate.result_type),
            Ok(pb::SearchResultType::Post | pb::SearchResultType::Journey)
        ) {
            continue;
        }
        let id = candidate.id.trim();
        if id.is_empty() || id != candidate.id {
            return Err(SearchMainError::InvalidContentSummary);
        }
        ids.insert(id.to_string());
    }
    Ok(ids)
}

fn reconcile_pending_results(
    candidates: Vec<pb::SearchResult>,
    summaries: bbs_link_pb::PublicContentSummaries,
) -> Result<Vec<pb::SearchResult>, SearchMainError> {
    let requested = pending_content_ids(&candidates)?;
    let mut authoritative = HashMap::with_capacity(summaries.items.len());
    for summary in summaries.items {
        let Some(post) = summary.post.as_ref() else {
            return Err(SearchMainError::InvalidContentSummary);
        };
        let Ok(content_type) = bbs_link_pb::ContentType::try_from(summary.content_type) else {
            return Err(SearchMainError::InvalidContentSummary);
        };
        if summary.id.is_empty()
            || summary.id != post.id
            || !requested.contains(&summary.id)
            || bbs_link_pb::GrowthDomain::try_from(post.domain).is_err()
            || post.is_route != (content_type == bbs_link_pb::ContentType::Route)
            || authoritative.insert(summary.id.clone(), summary).is_some()
        {
            return Err(SearchMainError::InvalidContentSummary);
        }
    }

    Ok(candidates
        .into_iter()
        .filter_map(
            |candidate| match pb::SearchResultType::try_from(candidate.result_type) {
                Ok(pb::SearchResultType::Post | pb::SearchResultType::Journey) => authoritative
                    .get(&candidate.id)
                    .map(|summary| search_result_from_summary(candidate, summary)),
                // User and topic candidates do not represent a public content item.
                _ => Some(candidate),
            },
        )
        .collect())
}

fn search_result_from_summary(
    candidate: pb::SearchResult,
    summary: &bbs_link_pb::PublicContentSummary,
) -> pb::SearchResult {
    let post = summary
        .post
        .as_ref()
        .expect("authoritative summaries are validated before rebuilding results");
    pb::SearchResult {
        id: summary.id.clone(),
        result_type: if summary.content_type == bbs_link_pb::ContentType::Route as i32 {
            pb::SearchResultType::Journey as i32
        } else {
            pb::SearchResultType::Post as i32
        },
        title: post.title.clone(),
        snippet: post.summary.clone(),
        cover_url: non_empty(&post.cover_url),
        author_id: Some(summary.author_id.clone()),
        author_name: Some(post.author_name.clone()),
        domain: Some(search_growth_domain(post.domain)),
        score: candidate.score,
        highlights: Vec::new(),
        post: Some(search_post_summary(post.clone())),
    }
}

fn search_post_summary(value: bbs_link_pb::PostSummary) -> pb::PostSummary {
    pb::PostSummary {
        id: value.id,
        author_name: value.author_name,
        author_avatar_url: value.author_avatar_url,
        title: value.title,
        summary: value.summary,
        domain: search_growth_domain(value.domain),
        cover_url: value.cover_url,
        route_title: value.route_title,
        route_duration: value.route_duration,
        join_count: value.join_count,
        like_count: value.like_count,
        freshness: value.freshness,
        tags: value.tags,
        is_route: value.is_route,
    }
}

fn search_growth_domain(value: i32) -> i32 {
    match bbs_link_pb::GrowthDomain::try_from(value) {
        Ok(bbs_link_pb::GrowthDomain::Learning) => pb::GrowthDomain::Learning as i32,
        Ok(bbs_link_pb::GrowthDomain::Movement) => pb::GrowthDomain::Movement as i32,
        Ok(bbs_link_pb::GrowthDomain::Wellness) => pb::GrowthDomain::Wellness as i32,
        Ok(bbs_link_pb::GrowthDomain::Travel) => pb::GrowthDomain::Travel as i32,
        Ok(bbs_link_pb::GrowthDomain::Leisure) | Err(_) => pb::GrowthDomain::Unspecified as i32,
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn make_search_plan(
    query: &str,
    dictionary: &QueryRewriteDictionary,
) -> Result<SearchPlan, SearchMainError> {
    let original_query = normalize_query(query)?;
    let mut recalls = vec![RecallPlan {
        query: original_query.clone(),
    }];
    let intent = search_intent(&original_query);
    let aliases = matches!(intent, SearchIntent::Generic | SearchIntent::Journey)
        .then(|| synonym_aliases(&original_query, dictionary))
        .unwrap_or_default();
    if !aliases.is_empty() {
        let mut expansion_terms = vec![original_query.clone()];
        expansion_terms.extend(aliases);
        recalls.push(RecallPlan {
            query: expansion_terms.join(" "),
        });
    }
    Ok(SearchPlan {
        intent,
        original_query,
        recalls,
        query_rewrite_version: dictionary.version.clone(),
    })
}

fn normalize_query(query: &str) -> Result<String, SearchMainError> {
    let query = query
        .chars()
        .map(|character| match character {
            '，' | '、' | '；' | ';' => ' ',
            character => character,
        })
        .collect::<String>();
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Err(SearchMainError::EmptyQuery);
    }
    if query.chars().count() > MAX_QUERY_LENGTH {
        return Err(SearchMainError::QueryTooLong);
    }
    Ok(query)
}

fn sanitize_dictionary(dictionary: QueryRewriteDictionary) -> Option<QueryRewriteDictionary> {
    if !valid_rewrite_version(&dictionary.version) {
        return None;
    }
    let mut seen_triggers = BTreeSet::new();
    let mut rules = dictionary
        .rules
        .into_iter()
        .filter_map(|rule| {
            let trigger = normalize_query(&rule.trigger).ok()?;
            if trigger.chars().count() > 32 || !seen_triggers.insert(trigger.to_lowercase()) {
                return None;
            }
            let trigger_key = trigger.to_lowercase();
            let mut seen_terms = BTreeSet::new();
            let expansion_terms = rule
                .expansion_terms
                .into_iter()
                .filter_map(|term| normalize_query(&term).ok())
                .filter(|term| term.chars().count() <= 32)
                .filter(|term| term.to_lowercase() != trigger_key)
                .filter(|term| seen_terms.insert(term.to_lowercase()))
                .take(MAX_REWRITE_TERMS)
                .collect::<Vec<_>>();
            (!expansion_terms.is_empty()).then_some(crate::datasource::QueryRewriteRule {
                trigger,
                expansion_terms,
            })
        })
        .collect::<Vec<_>>();
    rules.sort_by(|left, right| {
        right
            .trigger
            .chars()
            .count()
            .cmp(&left.trigger.chars().count())
            .then_with(|| left.trigger.cmp(&right.trigger))
    });
    if rules.is_empty() {
        return None;
    }
    Some(QueryRewriteDictionary {
        version: dictionary.version,
        rules,
    })
}

fn valid_rewrite_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn synonym_aliases(query: &str, dictionary: &QueryRewriteDictionary) -> Vec<String> {
    let query_key = query.to_lowercase();
    let mut aliases = Vec::new();
    for rule in &dictionary.rules {
        if !query_key.contains(&rule.trigger.to_lowercase()) {
            continue;
        }
        for alias in &rule.expansion_terms {
            let alias_key = alias.to_lowercase();
            let expansion_length = query.chars().count()
                + aliases
                    .iter()
                    .map(|term: &String| term.chars().count() + 1)
                    .sum::<usize>()
                + alias.chars().count()
                + 1;
            if !query_key.contains(&alias_key)
                && !aliases
                    .iter()
                    .any(|existing: &String| existing.to_lowercase() == alias_key)
                && aliases.len() < MAX_REWRITE_TERMS
                && expansion_length <= MAX_QUERY_LENGTH
            {
                aliases.push(alias.clone());
            }
        }
    }
    aliases
}

fn search_intent(query: &str) -> SearchIntent {
    if query.starts_with('@') {
        SearchIntent::User
    } else if query.starts_with('#') || query.contains("话题") {
        SearchIntent::Topic
    } else if ["路线", "计划", "挑战"]
        .iter()
        .any(|term| query.contains(term))
    {
        SearchIntent::Journey
    } else {
        SearchIntent::Generic
    }
}

fn new_session(fingerprint: u64, plan: &SearchPlan) -> SearchPipelineSession {
    SearchPipelineSession {
        query_fingerprint: fingerprint,
        query_rewrite_version: plan.query_rewrite_version.clone(),
        recalls: plan
            .recalls
            .iter()
            .map(|recall| RecallState {
                query: recall.query.clone(),
                source_cursor: None,
                exhausted: false,
            })
            .collect(),
        pending: Vec::new(),
        seen_result_ids: HashSet::new(),
        delivered_count: 0,
        source_total_estimate: 0,
        degraded: false,
    }
}

fn legacy_session(fingerprint: u64, query: String, source_cursor: String) -> SearchPipelineSession {
    SearchPipelineSession {
        query_fingerprint: fingerprint,
        query_rewrite_version: "legacy-unversioned".to_string(),
        recalls: vec![RecallState {
            query,
            source_cursor: Some(source_cursor),
            exhausted: false,
        }],
        pending: Vec::new(),
        seen_result_ids: HashSet::new(),
        delivered_count: 0,
        source_total_estimate: 0,
        degraded: false,
    }
}

fn all_recalls_exhausted(session: &SearchPipelineSession) -> bool {
    session.recalls.iter().all(|recall| recall.exhausted)
}

fn merge_candidates(
    pending: &mut Vec<pb::SearchResult>,
    seen_result_ids: &mut HashSet<String>,
    candidates: Vec<pb::SearchResult>,
) {
    for candidate in candidates {
        let identity = result_identity(&candidate);
        if seen_result_ids.insert(identity) {
            pending.push(candidate);
            continue;
        }
        if let Some(existing) = pending
            .iter_mut()
            .find(|existing| result_identity(existing) == result_identity(&candidate))
            && candidate.score > existing.score
        {
            *existing = candidate;
        }
    }
}

fn rerank_results(items: &mut [pb::SearchResult], query: &str, intent: SearchIntent) {
    let query = query.to_lowercase();
    let terms = query.split_whitespace().collect::<Vec<_>>();
    let expected_domain = domain_for_query(&query);
    for item in items.iter_mut() {
        let title = item.title.to_lowercase();
        if title.contains(&query) {
            item.score += 4.0;
        }
        let coverage = terms.iter().filter(|term| title.contains(**term)).count();
        item.score += coverage as f64 * 0.75;
        let matches_intent = match intent {
            SearchIntent::Generic => false,
            SearchIntent::Topic => item.result_type == pb::SearchResultType::Topic as i32,
            SearchIntent::User => item.result_type == pb::SearchResultType::User as i32,
            SearchIntent::Journey => item.result_type == pb::SearchResultType::Journey as i32,
        };
        if matches_intent {
            item.score += 2.0;
        }
        if expected_domain.is_some_and(|domain| item.domain == Some(domain)) {
            item.score += 0.5;
        }
    }
    sort_results(items);
}

fn sort_results(items: &mut [pb::SearchResult]) {
    items.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| {
                result_type_name(left.result_type).cmp(result_type_name(right.result_type))
            })
    });
}

fn domain_for_query(query: &str) -> Option<i32> {
    if ["跑步", "慢跑", "晨跑", "夜跑", "徒步", "登山", "步道"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(pb::GrowthDomain::Movement as i32)
    } else if ["阅读", "读书", "书单", "学习"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(pb::GrowthDomain::Learning as i32)
    } else if ["睡眠", "早睡", "冥想", "正念", "作息"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(pb::GrowthDomain::Wellness as i32)
    } else if ["旅行", "城市漫游", "出行"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(pb::GrowthDomain::Travel as i32)
    } else {
        None
    }
}

fn result_identity(item: &pb::SearchResult) -> String {
    format!("{}:{}", result_type_name(item.result_type), item.id)
}

fn result_type_name(result_type: i32) -> &'static str {
    match pb::SearchResultType::try_from(result_type) {
        Ok(pb::SearchResultType::Post) => "post",
        Ok(pb::SearchResultType::Journey) => "journey",
        Ok(pb::SearchResultType::User) => "user",
        Ok(pb::SearchResultType::Topic) | Err(_) => "topic",
    }
}

fn make_cursor(fingerprint: u64, session_id: &str) -> String {
    format!("sm1-{fingerprint:016x}-{session_id}")
}

fn parse_cursor(
    cursor: Option<&str>,
    fingerprint: u64,
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
) -> Result<Option<PublicCursor>, SearchMainError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    if cursor.len() > MAX_PUBLIC_CURSOR_BYTES {
        return Err(invalid_cursor("搜索游标无效"));
    }
    if let Some(value) = cursor.strip_prefix("sm1-") {
        let Some((cursor_fingerprint, session_id)) = value.split_once('-') else {
            return Err(invalid_cursor("搜索游标无效"));
        };
        if cursor_fingerprint != format!("{fingerprint:016x}") {
            return Err(invalid_cursor("搜索游标与当前查询不匹配"));
        }
        if uuid::Uuid::parse_str(session_id).is_err() {
            return Err(invalid_cursor("搜索游标无效"));
        }
        return Ok(Some(PublicCursor::New(session_id.to_string())));
    }
    if cursor.starts_with("v3-") {
        validate_legacy_cursor(cursor, query, search_type, viewer_id, excluded_author_ids)?;
        return Ok(Some(PublicCursor::Legacy(cursor.to_string())));
    }
    Err(invalid_cursor("搜索游标已过期，请重新搜索"))
}

fn validate_legacy_cursor(
    cursor: &str,
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
) -> Result<(), SearchMainError> {
    let Some(value) = cursor.strip_prefix("v3-") else {
        return Err(invalid_cursor("搜索游标无效"));
    };
    let Some((cursor_fingerprint, session_id)) = value.split_once('-') else {
        return Err(invalid_cursor("搜索游标无效"));
    };
    let expected = format!(
        "{:016x}",
        query_fingerprint(query, search_type, viewer_id, excluded_author_ids)
    );
    if cursor_fingerprint != expected {
        return Err(invalid_cursor("搜索游标与当前查询不匹配"));
    }
    if uuid::Uuid::parse_str(session_id).is_err() {
        return Err(invalid_cursor("搜索游标无效"));
    }
    Ok(())
}

fn invalid_cursor(message: &str) -> SearchMainError {
    SearchMainError::InvalidCursor(message.to_string())
}

fn query_fingerprint(
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
) -> u64 {
    stable_hash(&format!(
        "{}\0{}\0{}\0{}",
        search_type_name(search_type),
        query.to_lowercase(),
        viewer_id.unwrap_or_default(),
        excluded_author_ids.join("\0"),
    ))
}

fn normalize_excluded_author_ids(author_ids: &[String]) -> Vec<String> {
    author_ids
        .iter()
        .map(|author_id| author_id.trim())
        .filter(|author_id| !author_id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn search_type_name(search_type: pb::SearchType) -> &'static str {
    match search_type {
        pb::SearchType::All => "all",
        pb::SearchType::Posts => "posts",
        pb::SearchType::Journeys => "journeys",
        pb::SearchType::Users => "users",
        pb::SearchType::Topics => "topics",
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

    use std::{collections::HashMap, net::TcpListener, sync::Arc, time::Duration};

    use bookway_bbs_link_api::pb::{
        self as bbs_link_pb,
        bbs_link_client::BbsLinkClient,
        bbs_link_server::{BbsLink, BbsLinkServer},
    };
    use bookway_bbs_search_api::pb::{
        bbs_search_client::BbsSearchClient,
        bbs_search_server::{BbsSearch, BbsSearchServer},
    };
    use tokio::{sync::Mutex, time::sleep};
    use tonic::{Request, Response, Status};

    use super::*;

    type ResponseKey = (String, Option<String>);
    type RecordedResponse = Result<pb::SearchResponse, String>;

    #[derive(Clone, Default)]
    struct RecordingSearchSource {
        requests: Arc<Mutex<Vec<pb::SearchRequest>>>,
        responses: Arc<Mutex<HashMap<ResponseKey, RecordedResponse>>>,
        search_delay: Arc<Mutex<Option<Duration>>>,
    }

    #[derive(Clone, Default)]
    struct RecordingContentSource {
        summaries: Arc<Mutex<HashMap<String, bbs_link_pb::PublicContentSummary>>>,
        requests: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl RecordingContentSource {
        async fn publish(&self, summary: bbs_link_pb::PublicContentSummary) {
            self.summaries
                .lock()
                .await
                .insert(summary.id.clone(), summary);
        }
    }

    impl RecordingSearchSource {
        async fn respond(&self, query: &str, cursor: Option<&str>, response: pb::SearchResponse) {
            self.responses.lock().await.insert(
                (query.to_string(), cursor.map(str::to_string)),
                Ok(response),
            );
        }

        async fn fail(&self, query: &str, error: &str) {
            self.responses
                .lock()
                .await
                .insert((query.to_string(), None), Err(error.to_string()));
        }

        async fn delay_search(&self, delay: Duration) {
            self.search_delay.lock().await.replace(delay);
        }
    }

    #[tonic::async_trait]
    impl BbsSearch for RecordingSearchSource {
        async fn search(
            &self,
            request: Request<pb::SearchRequest>,
        ) -> Result<Response<pb::SearchResponse>, Status> {
            let request = request.into_inner();
            self.requests.lock().await.push(request.clone());
            if let Some(delay) = *self.search_delay.lock().await {
                sleep(delay).await;
            }
            let response = self
                .responses
                .lock()
                .await
                .get(&(request.q, request.cursor))
                .cloned()
                .unwrap_or_else(|| Ok(response(Vec::new(), None)));
            match response {
                Ok(response) => Ok(Response::new(response)),
                Err(error) => Err(Status::unavailable(error)),
            }
        }

        async fn suggestions(
            &self,
            request: Request<pb::SuggestionsRequest>,
        ) -> Result<Response<pb::SuggestionsResponse>, Status> {
            let request = request.into_inner();
            Ok(Response::new(pb::SuggestionsResponse {
                query: request.q,
                items: Vec::new(),
            }))
        }
    }

    #[tonic::async_trait]
    impl BbsLink for RecordingContentSource {
        async fn list(
            &self,
            _request: Request<bbs_link_pb::ListRequest>,
        ) -> Result<Response<bbs_link_pb::ContentPage>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn get_public_summaries(
            &self,
            request: Request<bbs_link_pb::PublicContentSummariesRequest>,
        ) -> Result<Response<bbs_link_pb::PublicContentSummaries>, Status> {
            let ids = request.into_inner().ids;
            self.requests.lock().await.push(ids.clone());
            let summaries = self.summaries.lock().await;
            Ok(Response::new(bbs_link_pb::PublicContentSummaries {
                items: ids
                    .into_iter()
                    .filter_map(|id| summaries.get(&id).cloned())
                    .collect(),
            }))
        }

        async fn get(
            &self,
            _request: Request<bbs_link_pb::IdRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn get_public(
            &self,
            _request: Request<bbs_link_pb::IdRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn create(
            &self,
            _request: Request<bbs_link_pb::CreateRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn update(
            &self,
            _request: Request<bbs_link_pb::UpdateRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn publish(
            &self,
            _request: Request<bbs_link_pb::PublishRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn restrict(
            &self,
            _request: Request<bbs_link_pb::RestrictRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn restore(
            &self,
            _request: Request<bbs_link_pb::RestoreRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }
    }

    async fn service(source: Arc<RecordingSearchSource>) -> Domain {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("allocate test server port");
        let address = listener.local_addr().expect("read test server address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(BbsSearchServer::new((*source).clone()))
                .serve(address)
                .await
                .expect("run bbs-search test server");
        });

        let endpoint = format!("http://{address}");
        for _ in 0..20 {
            if let Ok(search_client) = BbsSearchClient::connect(endpoint.clone()).await {
                return Domain::with_test_dependencies(
                    search_client,
                    Arc::new(MemorySearchSessionStore::default()),
                    Arc::new(MemorySearchExposureStore::default()),
                );
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("connect to bbs-search test server");
    }

    async fn content_client(
        source: RecordingContentSource,
    ) -> BbsLinkClient<tonic::transport::Channel> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("allocate test server port");
        let address = listener.local_addr().expect("read test server address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(BbsLinkServer::new(source))
                .serve(address)
                .await
                .expect("run bbs-link test server");
        });

        let endpoint = format!("http://{address}");
        for _ in 0..20 {
            if let Ok(client) = BbsLinkClient::connect(endpoint.clone()).await {
                return client;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("connect to bbs-link test server");
    }

    fn request(query: &str) -> pb::SearchRequest {
        pb::SearchRequest {
            q: query.to_string(),
            search_type: pb::SearchType::All as i32,
            limit: Some(20),
            ..Default::default()
        }
    }

    fn item(id: &str, title: &str, score: f64) -> pb::SearchResult {
        pb::SearchResult {
            id: id.to_string(),
            result_type: pb::SearchResultType::Post as i32,
            title: title.to_string(),
            snippet: String::new(),
            cover_url: None,
            author_id: None,
            author_name: None,
            domain: Some(pb::GrowthDomain::Movement as i32),
            score,
            highlights: Vec::new(),
            post: None,
        }
    }

    fn response(items: Vec<pb::SearchResult>, next_cursor: Option<&str>) -> pb::SearchResponse {
        pb::SearchResponse {
            request_id: String::new(),
            query: String::new(),
            total_estimate: u64::try_from(items.len()).unwrap_or(u64::MAX),
            items,
            next_cursor: next_cursor.map(str::to_string),
            took_ms: 0,
            degraded: false,
        }
    }

    fn public_summary(
        id: &str,
        author_id: &str,
        content_type: bbs_link_pb::ContentType,
        title: &str,
        summary: &str,
        domain: bbs_link_pb::GrowthDomain,
    ) -> bbs_link_pb::PublicContentSummary {
        bbs_link_pb::PublicContentSummary {
            id: id.to_string(),
            post: Some(bbs_link_pb::PostSummary {
                id: id.to_string(),
                author_name: format!("当前作者-{id}"),
                author_avatar_url: format!("https://cdn.example/{id}.png"),
                title: title.to_string(),
                summary: summary.to_string(),
                domain: domain as i32,
                cover_url: format!("https://cdn.example/{id}.jpg"),
                route_title: "当前路线".to_string(),
                route_duration: "30 分钟".to_string(),
                join_count: 12,
                like_count: 34,
                freshness: 0.8,
                tags: vec!["当前标签".to_string()],
                is_route: content_type == bbs_link_pb::ContentType::Route,
            }),
            author_id: author_id.to_string(),
            content_type: content_type as i32,
            topics: vec!["当前话题".to_string()],
            quality_score: 0.9,
        }
    }

    #[test]
    fn normalizes_whitespace_and_common_separators() {
        assert_eq!(
            normalize_query("  早晨，跑步； 阅读  ").expect("query should normalize"),
            "早晨 跑步 阅读"
        );
    }

    #[test]
    fn rejects_empty_and_oversized_queries() {
        assert!(matches!(
            normalize_query("  ").expect_err("empty query should be rejected"),
            SearchMainError::EmptyQuery
        ));
        assert!(matches!(
            normalize_query(&"x".repeat(101)).expect_err("long query should be rejected"),
            SearchMainError::QueryTooLong
        ));
    }

    #[test]
    fn pending_revalidation_drops_unpublished_content_and_rebuilds_public_fields() {
        let mut stale_post = item("post-1", "索引旧标题", 4.2);
        stale_post.snippet = "索引旧摘要".to_string();
        stale_post.cover_url = Some("https://stale.example/post.jpg".to_string());
        stale_post.author_id = Some("stale-author".to_string());
        stale_post.author_name = Some("索引旧作者".to_string());
        stale_post.highlights = vec!["索引命中".to_string()];
        let mut restricted = item("post-restricted", "不该显示", 3.0);
        restricted.highlights = vec!["旧命中".to_string()];
        let mut stale_route = item("route-1", "索引旧路线", 2.5);
        stale_route.result_type = pb::SearchResultType::Journey as i32;
        stale_route.highlights = vec!["旧路线命中".to_string()];
        let user = pb::SearchResult {
            id: "user-1".to_string(),
            result_type: pb::SearchResultType::User as i32,
            title: "不应变更的用户".to_string(),
            snippet: "用户摘要".to_string(),
            cover_url: Some("https://example/user.png".to_string()),
            author_id: Some("user-1".to_string()),
            author_name: Some("不应变更的用户".to_string()),
            domain: None,
            score: 1.5,
            highlights: vec!["用户命中".to_string()],
            post: None,
        };
        let topic = pb::SearchResult {
            id: "topic:跑步".to_string(),
            result_type: pb::SearchResultType::Topic as i32,
            title: "跑步".to_string(),
            snippet: "话题摘要".to_string(),
            cover_url: None,
            author_id: None,
            author_name: None,
            domain: Some(pb::GrowthDomain::Movement as i32),
            score: 1.2,
            highlights: vec!["话题命中".to_string()],
            post: None,
        };

        let reconciled = reconcile_pending_results(
            vec![
                stale_post,
                restricted,
                stale_route,
                user.clone(),
                topic.clone(),
            ],
            bbs_link_pb::PublicContentSummaries {
                items: vec![
                    public_summary(
                        "route-1",
                        "route-author",
                        bbs_link_pb::ContentType::Route,
                        "当前路线标题",
                        "当前路线摘要",
                        bbs_link_pb::GrowthDomain::Travel,
                    ),
                    public_summary(
                        "post-1",
                        "post-author",
                        bbs_link_pb::ContentType::Article,
                        "当前帖子标题",
                        "当前帖子摘要",
                        bbs_link_pb::GrowthDomain::Learning,
                    ),
                ],
            },
        )
        .expect("authoritative summaries should reconcile");

        assert_eq!(
            reconciled
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect::<Vec<_>>(),
            vec!["post-1", "route-1", "user-1", "topic:跑步"]
        );
        let post = &reconciled[0];
        assert_eq!(post.result_type, pb::SearchResultType::Post as i32);
        assert_eq!(post.title, "当前帖子标题");
        assert_eq!(post.snippet, "当前帖子摘要");
        assert_eq!(post.author_id.as_deref(), Some("post-author"));
        assert_eq!(post.score, 4.2, "the recall rank remains intact");
        assert!(post.highlights.is_empty(), "cached highlights are stale");
        assert_eq!(
            post.post.as_ref().map(|value| value.cover_url.as_str()),
            Some("https://cdn.example/post-1.jpg")
        );
        let route = &reconciled[1];
        assert_eq!(route.result_type, pb::SearchResultType::Journey as i32);
        assert_eq!(route.title, "当前路线标题");
        assert_eq!(route.score, 2.5, "the recall rank remains intact");
        assert!(route.highlights.is_empty());
        assert_eq!(reconciled[2], user);
        assert_eq!(reconciled[3], topic);
    }

    #[test]
    fn pending_revalidation_rejects_malformed_authoritative_summary_batches() {
        let candidates = vec![item("post-1", "索引标题", 1.0)];
        let mut mismatched = public_summary(
            "post-1",
            "author-1",
            bbs_link_pb::ContentType::Article,
            "当前标题",
            "当前摘要",
            bbs_link_pb::GrowthDomain::Learning,
        );
        mismatched.post.as_mut().expect("post summary").id = "other-id".to_string();
        assert!(matches!(
            reconcile_pending_results(
                candidates.clone(),
                bbs_link_pb::PublicContentSummaries {
                    items: vec![mismatched]
                }
            ),
            Err(SearchMainError::InvalidContentSummary)
        ));

        let duplicate = public_summary(
            "post-1",
            "author-1",
            bbs_link_pb::ContentType::Article,
            "当前标题",
            "当前摘要",
            bbs_link_pb::GrowthDomain::Learning,
        );
        assert!(matches!(
            reconcile_pending_results(
                candidates.clone(),
                bbs_link_pb::PublicContentSummaries {
                    items: vec![duplicate.clone(), duplicate]
                }
            ),
            Err(SearchMainError::InvalidContentSummary)
        ));

        assert!(matches!(
            reconcile_pending_results(
                candidates,
                bbs_link_pb::PublicContentSummaries {
                    items: vec![public_summary(
                        "unrequested-post",
                        "author-2",
                        bbs_link_pb::ContentType::Article,
                        "当前标题",
                        "当前摘要",
                        bbs_link_pb::GrowthDomain::Learning,
                    )]
                }
            ),
            Err(SearchMainError::InvalidContentSummary)
        ));
    }

    #[tokio::test]
    async fn revalidation_refills_a_page_after_cached_content_is_removed() {
        let search_source = Arc::new(RecordingSearchSource::default());
        search_source
            .respond(
                "跑步",
                None,
                response(
                    vec![
                        item("restricted-post", "跑步已限制", 2.0),
                        item("public-post", "跑步公开", 1.0),
                    ],
                    None,
                ),
            )
            .await;
        search_source
            .respond("跑步 慢跑 晨跑 夜跑", None, response(Vec::new(), None))
            .await;
        let content_source = RecordingContentSource::default();
        content_source
            .publish(public_summary(
                "public-post",
                "author-public",
                bbs_link_pb::ContentType::Article,
                "当前公开标题",
                "当前公开摘要",
                bbs_link_pb::GrowthDomain::Movement,
            ))
            .await;

        let mut domain = service(search_source).await;
        domain.content_client = Some(content_client(content_source.clone()).await);
        let mut search_request = request("跑步");
        search_request.limit = Some(1);
        let page = domain
            .search(search_request)
            .await
            .expect("search succeeds");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "public-post");
        assert_eq!(page.items[0].title, "当前公开标题");
        assert!(page.next_cursor.is_none());
        assert_eq!(
            *content_source.requests.lock().await,
            vec![
                vec!["restricted-post".to_string()],
                vec!["public-post".to_string()],
            ]
        );
    }

    #[test]
    fn versioned_rewrites_are_bounded_and_skip_identity_queries() {
        let dictionary = QueryRewriteDictionary {
            version: "lifestyle-v3".to_string(),
            rules: vec![crate::datasource::QueryRewriteRule {
                trigger: "跑步".to_string(),
                expansion_terms: vec!["慢跑".to_string(), "晨跑".to_string()],
            }],
        };
        let plan = make_search_plan("跑步 计划", &dictionary).expect("query plan should build");
        assert_eq!(plan.query_rewrite_version, "lifestyle-v3");
        assert_eq!(plan.recalls.len(), 2);
        assert_eq!(plan.recalls[1].query, "跑步 计划 慢跑 晨跑");
        assert_eq!(new_session(1, &plan).query_rewrite_version, "lifestyle-v3");

        let identity_plan =
            make_search_plan("#跑步", &dictionary).expect("topic query plan should build");
        assert_eq!(identity_plan.recalls.len(), 1);
    }

    #[test]
    fn invalid_rewrite_dictionaries_fall_back_instead_of_broadening_queries() {
        let invalid = QueryRewriteDictionary {
            version: "bad version".to_string(),
            rules: vec![crate::datasource::QueryRewriteRule {
                trigger: "跑步".to_string(),
                expansion_terms: vec!["慢跑".to_string()],
            }],
        };
        assert!(sanitize_dictionary(invalid).is_none());

        let empty = QueryRewriteDictionary {
            version: "empty-v1".to_string(),
            rules: Vec::new(),
        };
        assert!(sanitize_dictionary(empty).is_none());
    }

    struct FailingQueryRewriteRepository;

    #[async_trait::async_trait]
    impl crate::datasource::QueryRewriteRepository for FailingQueryRewriteRepository {
        async fn active(
            &self,
        ) -> Result<Option<QueryRewriteDictionary>, crate::datasource::QueryRewriteError> {
            Err(crate::datasource::QueryRewriteError::Storage(
                "configuration database unavailable".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn rewrite_cache_falls_back_to_a_known_dictionary_when_configuration_fails() {
        let cache = QueryRewriteCache::new(Arc::new(FailingQueryRewriteRepository));

        let resolution = cache.active().await;

        assert!(resolution.degraded);
        assert_eq!(resolution.dictionary.version, "builtin-v1");
        assert_eq!(
            make_search_plan("跑步", &resolution.dictionary)
                .expect("fallback dictionary remains usable")
                .recalls
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn recalls_canonical_and_known_synonym_queries() {
        let source = Arc::new(RecordingSearchSource::default());
        let service = service(source.clone()).await;

        service
            .search(request(" 跑步， "))
            .await
            .expect("search works");

        let queries = source
            .requests
            .lock()
            .await
            .iter()
            .map(|request| request.q.clone())
            .collect::<Vec<_>>();
        assert_eq!(queries, vec!["跑步", "跑步 慢跑 晨跑 夜跑"]);
    }

    #[tokio::test]
    async fn exact_recall_timeout_returns_a_bounded_upstream_error() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .delay_search(BBS_SEARCH_TIMEOUT + Duration::from_millis(20))
            .await;
        let service = service(source).await;

        assert!(matches!(
            service.search(request("跑步")).await,
            Err(SearchMainError::Upstream {
                code: tonic::Code::DeadlineExceeded,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn mixes_deduplicates_and_reranks_recall_candidates() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond(
                "跑步",
                None,
                response(
                    vec![
                        item("shared", "慢跑日记", 1.0),
                        item("exact", "跑步训练", 1.0),
                    ],
                    None,
                ),
            )
            .await;
        source
            .respond(
                "跑步 慢跑 晨跑 夜跑",
                None,
                response(
                    vec![
                        item("shared", "慢跑日记", 2.0),
                        item("expanded", "晨跑伙伴", 1.0),
                    ],
                    None,
                ),
            )
            .await;

        let result = service(source)
            .await
            .search(request("跑步"))
            .await
            .expect("search works");

        assert_eq!(result.items.len(), 3);
        assert_eq!(result.items[0].id, "exact");
        assert_eq!(
            result
                .items
                .iter()
                .filter(|item| item.id == "shared")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn search_response_request_id_validates_its_served_result() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond(
                "阅读",
                None,
                response(vec![item("post-1", "阅读笔记", 1.0)], None),
            )
            .await;
        let service = service(source).await;
        let mut search_request = request("阅读");
        search_request.user_id = Some("user-1".to_string());
        search_request.session_id = Some("session-1".to_string());

        let search_response = service.search(search_request).await.expect("search works");
        assert!(!search_response.request_id.is_empty());

        let validation = service
            .validate_attributions(api_pb::ValidateSearchAttributionsRequest {
                user_id: "user-1".to_string(),
                attributions: vec![api_pb::SearchAttribution {
                    request_id: search_response.request_id,
                    session_id: "session-1".to_string(),
                    result_id: search_response.items[0].id.clone(),
                    position: 0,
                }],
            })
            .await
            .expect("served result validates");

        assert_eq!(validation.valid, [true]);
    }

    #[tokio::test]
    async fn rejects_attribution_positions_outside_the_persistence_range() {
        let service = service(Arc::new(RecordingSearchSource::default())).await;

        let error = service
            .validate_attributions(api_pb::ValidateSearchAttributionsRequest {
                user_id: "user-1".to_string(),
                attributions: vec![api_pb::SearchAttribution {
                    request_id: "request-1".to_string(),
                    session_id: "session-1".to_string(),
                    result_id: "post-1".to_string(),
                    position: i32::MAX as u32 + 1,
                }],
            })
            .await
            .expect_err("oversized attribution positions are invalid");

        assert!(matches!(error, SearchExposureError::PositionOutOfRange));
    }

    #[tokio::test]
    async fn next_page_drains_pending_candidates_without_redelivery() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond(
                "跑步",
                None,
                response(
                    vec![item("a", "跑步 a", 1.0), item("b", "跑步 b", 1.0)],
                    None,
                ),
            )
            .await;
        source
            .respond(
                "跑步 慢跑 晨跑 夜跑",
                None,
                response(
                    vec![item("c", "晨跑 c", 1.0), item("d", "夜跑 d", 1.0)],
                    None,
                ),
            )
            .await;
        let service = service(source).await;
        let mut first_request = request("跑步");
        first_request.limit = Some(2);
        let first = service
            .search(first_request)
            .await
            .expect("first page works");
        let cursor = first
            .next_cursor
            .clone()
            .expect("pending results keep cursor");
        let mut second_request = request("跑步");
        second_request.limit = Some(2);
        second_request.cursor = Some(cursor);
        let second = service
            .search(second_request)
            .await
            .expect("second page works");

        let first_ids = first
            .items
            .into_iter()
            .map(|item| item.id)
            .collect::<HashSet<_>>();
        let second_ids = second
            .items
            .into_iter()
            .map(|item| item.id)
            .collect::<HashSet<_>>();
        assert!(first_ids.is_disjoint(&second_ids));
        assert_eq!(first_ids.len() + second_ids.len(), 4);
        assert!(second.next_cursor.is_none());
    }

    #[tokio::test]
    async fn public_cursor_rejects_changed_visibility_or_query_context() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond(
                "跑步",
                None,
                response(vec![item("a", "跑步", 1.0), item("b", "跑步", 1.0)], None),
            )
            .await;
        source
            .respond("跑步 慢跑 晨跑 夜跑", None, response(Vec::new(), None))
            .await;
        let service = service(source).await;
        let mut first_request = request("跑步");
        first_request.limit = Some(1);
        first_request.user_id = Some("viewer-a".to_string());
        first_request.excluded_author_ids = vec![" muted ".to_string()];
        let cursor = service
            .search(first_request)
            .await
            .expect("first page works")
            .next_cursor
            .expect("cursor exists");

        let mut changed = request("阅读");
        changed.cursor = Some(cursor.clone());
        assert!(matches!(
            service
                .search(changed)
                .await
                .expect_err("query must bind cursor"),
            SearchMainError::InvalidCursor(_)
        ));
        let mut changed = request("跑步");
        changed.cursor = Some(cursor.clone());
        changed.user_id = Some("viewer-b".to_string());
        assert!(matches!(
            service
                .search(changed)
                .await
                .expect_err("viewer must bind cursor"),
            SearchMainError::InvalidCursor(_)
        ));
        let mut changed = request("跑步");
        changed.cursor = Some(cursor.clone());
        changed.user_id = Some("viewer-a".to_string());
        changed.excluded_author_ids = vec!["other".to_string()];
        assert!(matches!(
            service
                .search(changed)
                .await
                .expect_err("visibility must bind cursor"),
            SearchMainError::InvalidCursor(_)
        ));
        let mut changed = request("跑步");
        changed.cursor = Some(cursor);
        changed.user_id = Some("viewer-a".to_string());
        changed.excluded_author_ids = vec!["muted".to_string()];
        changed.search_type = pb::SearchType::Posts as i32;
        assert!(matches!(
            service
                .search(changed)
                .await
                .expect_err("type must bind cursor"),
            SearchMainError::InvalidCursor(_)
        ));
    }

    #[tokio::test]
    async fn legacy_cursor_keeps_a_single_exact_recall_and_returns_sm1() {
        let source = Arc::new(RecordingSearchSource::default());
        let legacy_id = uuid::Uuid::now_v7();
        let fingerprint = query_fingerprint("跑步", pb::SearchType::All, None, &[]);
        let legacy_cursor = format!("v3-{fingerprint:016x}-{legacy_id}");
        source
            .respond(
                "跑步",
                Some(&legacy_cursor),
                response(vec![item("a", "跑步", 1.0)], Some("v3-next")),
            )
            .await;
        let service = service(source.clone()).await;
        let mut request = request("跑步");
        request.cursor = Some(legacy_cursor);
        request.limit = Some(1);
        let result = service.search(request).await.expect("legacy page works");

        assert!(
            result
                .next_cursor
                .as_deref()
                .is_some_and(|cursor| cursor.starts_with("sm1-"))
        );
        assert_eq!(source.requests.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn expansion_failure_keeps_exact_results_and_marks_degraded() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond("跑步", None, response(vec![item("a", "跑步", 1.0)], None))
            .await;
        source
            .fail("跑步 慢跑 晨跑 夜跑", "expansion unavailable")
            .await;

        let result = service(source)
            .await
            .search(request("跑步"))
            .await
            .expect("exact recall remains available");

        assert_eq!(result.items.len(), 1);
        assert!(result.degraded);
    }
}
