use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use bookway_bbs_link_api::pb::{self as bbs_link_pb, bbs_link_client::BbsLinkClient};
use bookway_bbs_search_api::pb;
use thiserror::Error;

use super::datasource::{
    EntityBias, MemorySearchAnalytics, MemorySearchSessionStore, OpenSearchSource,
    PostgresSearchAnalytics, PostgresSearchSessionStore, SearchSession, SearchSessionStore,
    SearchSource, SearchSourceError, SearchSourceResult, SharedSearchAnalytics, search_type_name,
    stable_hash,
};
use crate::conf::Config;

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;
const SOURCE_PAGE_SIZE: usize = 100;
const MAX_SOURCE_PAGES_PER_RESPONSE: usize = 20;
const MAX_PUBLIC_CURSOR_BYTES: usize = 128;
const MAX_QUERY_LENGTH: usize = 100;
const MAX_ROUTE_CONTEXT_FIELD_LENGTH: usize = 160;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    search: SearchService,
    content_client: BbsLinkClient<tonic::transport::Channel>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let content_client =
            BbsLinkClient::new(bookway_runtime::grpc_channel(&config.bbs_link_url).await?);
        let source: Option<Arc<dyn SearchSource>> = match config.opensearch_url.clone() {
            Some(url) => Some(Arc::new(OpenSearchSource::new(
                url,
                config.opensearch_read_alias.clone(),
            ))),
            None => None,
        };
        let (analytics, sessions): (SharedSearchAnalytics, Arc<dyn SearchSessionStore>) =
            match bookway_data::storage_mode()? {
                bookway_data::StorageMode::Memory => (
                    Arc::new(MemorySearchAnalytics::default()),
                    Arc::new(MemorySearchSessionStore::default()),
                ),
                bookway_data::StorageMode::Postgres => {
                    let pool = bookway_data::postgres_pool().await?;
                    (
                        Arc::new(PostgresSearchAnalytics::new(pool.clone())),
                        Arc::new(PostgresSearchSessionStore::new(pool)),
                    )
                }
            };
        Ok(Self {
            config,
            search: SearchService::with_dependencies(source, analytics, sessions),
            content_client,
        })
    }

    pub(crate) async fn search(
        &self,
        request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, SearchError> {
        self.search
            .search_with_content_client(request, Some(self.content_client.clone()))
            .await
    }

    pub(crate) async fn suggestions(
        &self,
        request: pb::SuggestionsRequest,
    ) -> Result<pb::SuggestionsResponse, SearchError> {
        self.search
            .suggestions_with_content_client(request, Some(self.content_client.clone()))
            .await
    }

    /// One-shot semantic recall used by Search Main as an additional lane.
    /// The query vector was produced by the same catalog embedding provider
    /// the indexer used; no vectors indexed yet simply means no results.
    pub(crate) async fn search_semantic(
        &self,
        request: pb::SearchSemanticRequest,
    ) -> Result<pb::SearchResponse, SearchError> {
        let started = Instant::now();
        let query_text = request.q.trim().to_string();
        if request.query_vector.is_empty() {
            return Err(SearchError::Validation(
                "query_vector is required for semantic search".to_string(),
            ));
        }
        let search_type = pb::SearchType::try_from(request.search_type.unwrap_or_default())
            .map_err(|_| SearchError::Validation("搜索类型无效".to_string()))?;
        if matches!(
            search_type,
            pb::SearchType::Users | pb::SearchType::Topics | pb::SearchType::Resources
        ) {
            return Err(SearchError::Validation(
                "语义搜索不支持该搜索类型".to_string(),
            ));
        }
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        let excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let excluded_authors = excluded_author_ids.iter().cloned().collect::<HashSet<_>>();
        let entity_bias = match search_type {
            pb::SearchType::Nodes => Some(EntityBias::ActionNode),
            pb::SearchType::Equipment => Some(EntityBias::SceneEquipment),
            _ => None,
        };
        let Some(source) = self.search.source.as_ref() else {
            return Ok(empty_semantic_response(&query_text, started));
        };
        let result = match source
            .search_semantic(
                &request.query_vector,
                limit,
                &excluded_author_ids,
                entity_bias,
            )
            .await
        {
            Ok(result) => result,
            Err(SearchSourceError::SemanticUnavailable) => {
                return Ok(empty_semantic_response(&query_text, started));
            }
            Err(error) => return Err(error.into()),
        };
        let items = search_results(
            &result.page.items,
            &query_text,
            search_type,
            &excluded_authors,
            result.source_ranked,
            None,
            true,
        );
        Ok(pb::SearchResponse {
            request_id: String::new(),
            query: query_text,
            items,
            next_cursor: None,
            total_estimate: result.page.total_estimate,
            took_ms: started.elapsed().as_millis() as u64,
            degraded: false,
        })
    }
}

fn empty_semantic_response(query: &str, started: Instant) -> pb::SearchResponse {
    pb::SearchResponse {
        request_id: String::new(),
        query: query.to_string(),
        items: Vec::new(),
        next_cursor: None,
        total_estimate: 0,
        took_ms: started.elapsed().as_millis() as u64,
        degraded: false,
    }
}

#[derive(Debug, Error)]
pub(crate) enum SearchError {
    #[error("{0}")]
    Validation(String),
    #[error("搜索会话已过期，请重新搜索")]
    CursorExpired,
    #[error(transparent)]
    Source(#[from] SearchSourceError),
}

#[derive(Clone)]
pub(crate) struct SearchService {
    source: Option<Arc<dyn SearchSource>>,
    analytics: SharedSearchAnalytics,
    sessions: Arc<dyn SearchSessionStore>,
    popular_terms: Arc<Vec<String>>,
}

impl SearchService {
    #[cfg(test)]
    pub(crate) fn new(source: Arc<dyn SearchSource>) -> Self {
        Self::with_dependencies(
            Some(source),
            Arc::new(MemorySearchAnalytics::default()),
            Arc::new(MemorySearchSessionStore::default()),
        )
    }

    pub(crate) fn with_dependencies(
        source: Option<Arc<dyn SearchSource>>,
        analytics: SharedSearchAnalytics,
        sessions: Arc<dyn SearchSessionStore>,
    ) -> Self {
        Self {
            source,
            analytics,
            sessions,
            popular_terms: Arc::new(vec![
                "主题阅读".to_string(),
                "晨跑".to_string(),
                "城市漫游".to_string(),
                "睡眠修复".to_string(),
                "周末手作".to_string(),
                "博物馆".to_string(),
            ]),
        }
    }

    #[cfg(test)]
    pub(crate) async fn search(
        &self,
        request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, SearchError> {
        self.search_with_content_client(request, None).await
    }

    async fn search_with_content_client(
        &self,
        request: pb::SearchRequest,
        content_client: Option<BbsLinkClient<tonic::transport::Channel>>,
    ) -> Result<pb::SearchResponse, SearchError> {
        let search_type = pb::SearchType::try_from(request.search_type)
            .map_err(|_| SearchError::Validation("搜索类型无效".to_string()))?;
        let query_text = request.q.trim().to_string();
        if query_text.is_empty() || query_text.chars().count() > MAX_QUERY_LENGTH {
            return Err(SearchError::Validation(
                "搜索词需要在 1 到 100 个字符之间".to_string(),
            ));
        }
        let excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let excluded_authors = excluded_author_ids.iter().cloned().collect::<HashSet<_>>();
        if request
            .route_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
            || request
                .action_node_id
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            || request
                .route_id
                .as_deref()
                .is_some_and(|value| value.trim().chars().count() > MAX_ROUTE_CONTEXT_FIELD_LENGTH)
            || request
                .action_node_id
                .as_deref()
                .is_some_and(|value| value.trim().chars().count() > MAX_ROUTE_CONTEXT_FIELD_LENGTH)
            || request.route_id.is_some() != request.action_node_id.is_some()
            || request
                .scene_equipment
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            || request
                .scene_equipment
                .as_deref()
                .is_some_and(|value| value.trim().chars().count() > MAX_ROUTE_CONTEXT_FIELD_LENGTH)
            || (request.scene_equipment.is_some()
                && (request.route_id.is_none() || request.action_node_id.is_none()))
        {
            return Err(SearchError::Validation(
                "route_id and action_node_id must be provided together".to_string(),
            ));
        }
        let route_context = route_context(&request);
        let entity_bias = match search_type {
            pb::SearchType::Nodes => Some(EntityBias::ActionNode),
            pb::SearchType::Equipment => Some(EntityBias::SceneEquipment),
            _ => None,
        };
        let fingerprint = query_fingerprint(
            &query_text,
            search_type,
            request.user_id.as_deref(),
            &excluded_author_ids,
            route_context.as_ref(),
        );
        let session_id = parse_cursor(
            request.cursor.as_deref(),
            &query_text,
            search_type,
            request.user_id.as_deref(),
            &excluded_author_ids,
            route_context.as_ref(),
        )?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        let started = Instant::now();
        let mut session = match session_id.as_deref() {
            Some(id) => self
                .sessions
                .load(id)
                .await?
                .filter(|session| session.query_fingerprint == fingerprint)
                .ok_or(SearchError::CursorExpired)?,
            None => SearchSession {
                query_fingerprint: fingerprint,
                source_cursor: None,
                source_exhausted: false,
                pending: Vec::new(),
                seen_result_ids: HashSet::new(),
                delivered_count: 0,
                source_total_estimate: 0,
                degraded: false,
            },
        };
        let mut page = Vec::with_capacity(limit);
        let mut source_pages = 0;
        while page.len() < limit {
            if !session.pending.is_empty() {
                let take = (limit - page.len()).min(session.pending.len());
                page.extend(session.pending.drain(..take));
                continue;
            }
            if session.source_exhausted || source_pages >= MAX_SOURCE_PAGES_PER_RESPONSE {
                break;
            }
            let source_query = bbs_link_pb::ListRequest {
                cursor: session.source_cursor.clone(),
                limit: Some(SOURCE_PAGE_SIZE as u32),
                status: Some(bbs_link_pb::ContentStatus::Published as i32),
                strategy: Some("fresh".to_string()),
                ids: route_context
                    .as_ref()
                    .map(|context| context.route_id.clone()),
                author_id: None,
                content_type: match search_type {
                    pb::SearchType::Journeys | pb::SearchType::Nodes
                    | pb::SearchType::Equipment => {
                        Some(bbs_link_pb::ContentType::Route as i32)
                    }
                    _ => None,
                },
                domain: None,
                author_ids: Vec::new(),
            };
            let source_result = self
                .search_contents(
                    source_query,
                    &query_text,
                    &excluded_author_ids,
                    route_context.as_ref(),
                    entity_bias,
                    content_client.as_ref(),
                )
                .await
                .map_err(map_source_error)?;
            source_pages += 1;
            session.source_cursor = source_result.page.next_cursor;
            session.source_exhausted = session.source_cursor.is_none();
            session.source_total_estimate = session
                .source_total_estimate
                .max(usize::try_from(source_result.page.total_estimate).unwrap_or(usize::MAX));
            session.degraded |= source_result.degraded;
            let mut candidates = search_results(
                &source_result.page.items,
                &query_text,
                search_type,
                &excluded_authors,
                source_result.source_ranked,
                route_context.as_ref(),
                false,
            );
            if !source_result.source_ranked {
                sort_results(&mut candidates);
            }
            for item in candidates {
                if session.seen_result_ids.insert(result_identity(&item)) {
                    session.pending.push(item);
                }
            }
        }
        session.delivered_count += page.len();
        let has_next_page = !session.pending.is_empty() || !session.source_exhausted;
        let total_estimate = if session.source_exhausted {
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
                        return Err(SearchError::CursorExpired);
                    }
                    id.to_string()
                }
                None => self.sessions.create(session.clone()).await?,
            };
            Some(make_cursor(
                &query_text,
                search_type,
                request.user_id.as_deref(),
                &excluded_author_ids,
                route_context.as_ref(),
                &id,
            ))
        } else {
            if let Some(id) = session_id.as_deref() {
                self.sessions.delete(id).await?;
            }
            None
        };
        if session_id.is_none() {
            self.analytics
                .record(
                    request.user_id.as_deref(),
                    &query_text,
                    search_type,
                    !has_next_page && page.is_empty(),
                )
                .await;
        }
        Ok(pb::SearchResponse {
            request_id: String::new(),
            query: query_text,
            items: page,
            next_cursor,
            total_estimate: u64::try_from(total_estimate).unwrap_or(u64::MAX),
            took_ms: started.elapsed().as_millis() as u64,
            degraded: session.degraded,
        })
    }

    #[cfg(test)]
    pub(crate) async fn suggestions(
        &self,
        request: pb::SuggestionsRequest,
    ) -> Result<pb::SuggestionsResponse, SearchError> {
        self.suggestions_with_content_client(request, None).await
    }

    async fn suggestions_with_content_client(
        &self,
        request: pb::SuggestionsRequest,
        content_client: Option<BbsLinkClient<tonic::transport::Channel>>,
    ) -> Result<pb::SuggestionsResponse, SearchError> {
        let query = request.q.trim().to_string();
        if query.is_empty() || query.chars().count() > MAX_QUERY_LENGTH {
            return Ok(pb::SuggestionsResponse {
                query,
                items: Vec::new(),
            });
        }
        let excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let excluded_authors = excluded_author_ids.iter().cloned().collect::<HashSet<_>>();
        let source_query = bbs_link_pb::ListRequest {
            limit: Some(30),
            status: Some(bbs_link_pb::ContentStatus::Published as i32),
            strategy: Some("quality".to_string()),
            ..Default::default()
        };
        let (popular, source) = tokio::join!(
            self.analytics
                .suggestions(request.user_id.as_deref(), &query, 8),
            self.search_contents(
                source_query,
                &query,
                &excluded_author_ids,
                None,
                None,
                content_client.as_ref(),
            ),
        );
        let mut items = popular;
        if let Ok(source) = source {
            let next_cursor = source.page.next_cursor.clone();
            items.extend(content_suggestions(
                &source.page.items,
                &query,
                &excluded_authors,
            ));
            if let Some(cursor) = next_cursor
                && let Some(source) = &self.source
            {
                source.release_search_cursor(&cursor).await;
            }
        }
        let lower = query.to_lowercase();
        items.extend(
            self.popular_terms
                .iter()
                .filter(|term| term.to_lowercase().contains(&lower))
                .enumerate()
                .map(|(index, term)| pb::Suggestion {
                    text: term.clone(),
                    result_type: pb::SearchResultType::Topic as i32,
                    score: 0.2 / (index as f64 + 1.0),
                    personal: false,
                }),
        );
        deduplicate_suggestions(&mut items);
        items.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.text.cmp(&right.text))
        });
        items.truncate(8);
        Ok(pb::SuggestionsResponse { query, items })
    }

    async fn search_contents(
        &self,
        mut query: bbs_link_pb::ListRequest,
        text: &str,
        excluded_author_ids: &[String],
        route_context: Option<&RouteSearchContext>,
        entity_bias: Option<EntityBias>,
        content_client: Option<&BbsLinkClient<tonic::transport::Channel>>,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let is_fallback_cursor = query
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.strip_prefix("fallback:"))
            .map(str::to_string);
        if let Some(cursor) = is_fallback_cursor {
            query.cursor = Some(cursor);
            return self.search_bbs_link(query, content_client, true).await;
        }
        let Some(source) = &self.source else {
            // The BBS Link fallback has no biasable fields; typed extraction
            // happens domain-side on the plain content reads below.
            let _ = entity_bias;
            return self.search_bbs_link(query, content_client, false).await;
        };
        match source
            .search_contents(query.clone(), text, excluded_author_ids, entity_bias)
            .await
        {
            Ok(result) => {
                self.revalidate_indexed_contents(result, route_context, content_client)
                    .await
            }
            Err(SearchSourceError::Fallback) => {
                self.search_bbs_link(query, content_client, true).await
            }
            Err(error) => Err(error),
        }
    }

    async fn revalidate_indexed_contents(
        &self,
        result: SearchSourceResult,
        route_context: Option<&RouteSearchContext>,
        content_client: Option<&BbsLinkClient<tonic::transport::Channel>>,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        if !result.source_ranked || result.page.items.is_empty() {
            return Ok(result);
        }
        let content_client = content_client.ok_or_else(|| {
            SearchSourceError::Request(
                "bbs-link public summary client is unavailable for indexed results".to_string(),
            )
        })?;
        let ids = indexed_content_ids(&result.page.items)?;
        let mut client = content_client.clone();
        let summaries = client
            .get_public_summaries(
                bookway_runtime::grpc_service_request(bbs_link_pb::PublicContentSummariesRequest {
                    ids,
                })
                .map_err(|error| SearchSourceError::Request(error.to_string()))?,
            )
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?
            .into_inner();
        let (page, stale_index_hits) =
            reconcile_indexed_page(result.page, summaries, route_context)?;
        Ok(SearchSourceResult {
            page,
            // A stale index can no longer leak content, but it may underfill
            // this page until the outbox/reconciliation loop catches up.
            degraded: result.degraded || stale_index_hits,
            source_ranked: true,
        })
    }

    async fn search_bbs_link(
        &self,
        query: bbs_link_pb::ListRequest,
        content_client: Option<&BbsLinkClient<tonic::transport::Channel>>,
        degraded: bool,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let content_client = content_client.ok_or_else(|| {
            SearchSourceError::Request("bbs-link client is unavailable".to_string())
        })?;
        let mut client = content_client.clone();
        let response = client
            .list(
                bookway_runtime::grpc_service_request(query)
                    .map_err(|error| SearchSourceError::Request(error.to_string()))?,
            )
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?
            .into_inner();
        let page = response;
        Ok(SearchSourceResult {
            page: bbs_link_pb::ContentPage {
                next_cursor: page.next_cursor.map(|cursor| {
                    if degraded {
                        format!("fallback:{cursor}")
                    } else {
                        cursor
                    }
                }),
                ..page
            },
            degraded,
            source_ranked: false,
        })
    }
}

fn indexed_content_ids(items: &[bbs_link_pb::Content]) -> Result<Vec<String>, SearchSourceError> {
    let mut ids = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());
    for item in items {
        let id = item.id.trim();
        if id.is_empty() || id != item.id || !seen.insert(id.to_string()) {
            return Err(SearchSourceError::Request(
                "OpenSearch returned an invalid or duplicate content ID".to_string(),
            ));
        }
        ids.push(id.to_string());
    }
    Ok(ids)
}

fn reconcile_indexed_page(
    indexed: bbs_link_pb::ContentPage,
    summaries: bbs_link_pb::PublicContentSummaries,
    route_context: Option<&RouteSearchContext>,
) -> Result<(bbs_link_pb::ContentPage, bool), SearchSourceError> {
    let requested = indexed_content_ids(&indexed.items)?
        .into_iter()
        .collect::<HashSet<_>>();
    let mut authoritative = HashMap::with_capacity(summaries.items.len());
    for summary in summaries.items {
        let Some(post) = summary.post.as_ref() else {
            return Err(SearchSourceError::Request(
                "bbs-link returned a public summary without post metadata".to_string(),
            ));
        };
        let Ok(content_type) = bbs_link_pb::ContentType::try_from(summary.content_type) else {
            return Err(SearchSourceError::Request(
                "bbs-link returned a public summary with an invalid content type".to_string(),
            ));
        };
        if summary.id.is_empty()
            || summary.id != post.id
            || !requested.contains(&summary.id)
            || post.is_route != (content_type == bbs_link_pb::ContentType::Route)
            || post.is_milestone != (content_type == bbs_link_pb::ContentType::Milestone)
            || post.is_question != (content_type == bbs_link_pb::ContentType::Question)
            || authoritative.insert(summary.id.clone(), summary).is_some()
        {
            return Err(SearchSourceError::Request(
                "bbs-link returned an invalid public summary batch".to_string(),
            ));
        }
    }
    let indexed_count = indexed.items.len();
    let items = indexed
        .items
        .into_iter()
        .filter_map(|content| {
            let summary = authoritative.remove(&content.id)?;
            if route_context
                .is_some_and(|context| !route_matches_context_summary(&summary, context))
            {
                return None;
            }
            // The summary is intentionally compact, but public route action
            // nodes are part of its contract. Rebuild only that public
            // execution context so indexed and fallback hits have identical
            // action/equipment semantics without reintroducing private data.
            let route_template = (summary.content_type == bbs_link_pb::ContentType::Route as i32
                && !summary.route_actions.is_empty())
            .then(|| bbs_link_pb::RouteTemplate {
                actions: summary.route_actions.clone(),
                ..Default::default()
            });
            // The index supplies only candidate IDs and rank. Rebuild every
            // displayed field from the current BBS Link public projection.
            Some(bbs_link_pb::Content {
                id: summary.id,
                post: summary.post,
                author_id: summary.author_id,
                content_type: summary.content_type,
                status: bbs_link_pb::ContentStatus::Published as i32,
                topics: summary.topics,
                quality_score: summary.quality_score,
                route_template,
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    let stale_index_hits = items.len() != indexed_count;
    Ok((
        bbs_link_pb::ContentPage {
            items,
            next_cursor: indexed.next_cursor,
            total_estimate: indexed.total_estimate,
        },
        stale_index_hits,
    ))
}

fn make_cursor(
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
    route_context: Option<&RouteSearchContext>,
    session_id: &str,
) -> String {
    let fingerprint = query_fingerprint(
        query,
        search_type,
        viewer_id,
        excluded_author_ids,
        route_context,
    );
    format!("v3-{fingerprint:016x}-{session_id}")
}

fn parse_cursor(
    cursor: Option<&str>,
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
    route_context: Option<&RouteSearchContext>,
) -> Result<Option<String>, SearchError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    if cursor.len() > MAX_PUBLIC_CURSOR_BYTES {
        return Err(SearchError::Validation("搜索游标无效".to_string()));
    }
    let Some(value) = cursor.strip_prefix("v3-") else {
        return Err(SearchError::Validation(
            "搜索游标已过期，请重新搜索".to_string(),
        ));
    };
    let Some((fingerprint, session_id)) = value.split_once('-') else {
        return Err(SearchError::Validation("搜索游标无效".to_string()));
    };
    let expected = format!(
        "{:016x}",
        query_fingerprint(
            query,
            search_type,
            viewer_id,
            excluded_author_ids,
            route_context
        )
    );
    if fingerprint != expected {
        return Err(SearchError::Validation(
            "搜索游标与当前查询不匹配".to_string(),
        ));
    }
    if uuid::Uuid::parse_str(session_id).is_err() {
        return Err(SearchError::Validation("搜索游标无效".to_string()));
    }
    Ok(Some(session_id.to_string()))
}

fn map_source_error(error: SearchSourceError) -> SearchError {
    match error {
        SearchSourceError::CursorExpired => SearchError::CursorExpired,
        error => SearchError::Source(error),
    }
}

fn query_fingerprint(
    query: &str,
    search_type: pb::SearchType,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
    route_context: Option<&RouteSearchContext>,
) -> u64 {
    let context = route_context
        .map(|value| {
            format!(
                "{}\0{}\0{}",
                value.route_id, value.action_node_id, value.scene_equipment
            )
        })
        .unwrap_or_default();
    stable_hash(&format!(
        "{}\0{}\0{}\0{}\0{}",
        search_type_name(search_type),
        query.to_lowercase(),
        viewer_id.unwrap_or_default(),
        excluded_author_ids.join("\0"),
        context,
    ))
}

#[derive(Clone, Debug)]
struct RouteSearchContext {
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
}

fn route_context(request: &pb::SearchRequest) -> Option<RouteSearchContext> {
    let route_id = request.route_id.as_deref()?.trim();
    let action_node_id = request.action_node_id.as_deref()?.trim();
    let scene_equipment = request
        .scene_equipment
        .as_deref()
        .unwrap_or_default()
        .trim();
    if route_id.is_empty() || action_node_id.is_empty() {
        return None;
    }
    Some(RouteSearchContext {
        route_id: route_id.to_string(),
        action_node_id: action_node_id.to_string(),
        scene_equipment: scene_equipment.to_lowercase(),
    })
}

fn normalize_excluded_author_ids(author_ids: &[String]) -> Vec<String> {
    author_ids
        .iter()
        .map(|author_id| author_id.trim())
        .filter(|author_id| !author_id.is_empty())
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn content_suggestions(
    contents: &[bbs_link_pb::Content],
    query: &str,
    excluded_authors: &HashSet<String>,
) -> Vec<pb::Suggestion> {
    let query = query.to_lowercase();
    let mut items = Vec::new();
    for (content, post) in contents
        .iter()
        .filter(|content| !excluded_authors.contains(&content.author_id))
        .filter_map(|content| content.post.as_ref().map(|post| (content, post)))
    {
        let base = content.quality_score.clamp(0.0, 1.0);
        push_suggestion(
            &mut items,
            &query,
            &post.title,
            if content.content_type == bbs_link_pb::ContentType::Route as i32 {
                pb::SearchResultType::Journey
            } else {
                pb::SearchResultType::Post
            },
            1.5 + base,
        );
        push_suggestion(
            &mut items,
            &query,
            &post.author_name,
            pb::SearchResultType::User,
            0.8 + base,
        );
        for topic in post.tags.iter().chain(&content.topics) {
            push_suggestion(
                &mut items,
                &query,
                topic,
                pb::SearchResultType::Topic,
                1.0 + base,
            );
        }
        if content.content_type == bbs_link_pb::ContentType::Route as i32 {
            for action in content
                .route_template
                .iter()
                .flat_map(|template| &template.actions)
            {
                push_suggestion(
                    &mut items,
                    &query,
                    &action.title,
                    pb::SearchResultType::Journey,
                    1.2 + base,
                );
                for equipment in &action.scene_equipment {
                    push_suggestion(
                        &mut items,
                        &query,
                        equipment,
                        pb::SearchResultType::Journey,
                        1.1 + base,
                    );
                }
            }
        }
    }
    items
}

fn push_suggestion(
    items: &mut Vec<pb::Suggestion>,
    query: &str,
    text: &str,
    result_type: pb::SearchResultType,
    score: f64,
) {
    let lower = text.to_lowercase();
    if !text.trim().is_empty() && lower.contains(query) {
        items.push(pb::Suggestion {
            text: text.to_string(),
            result_type: result_type as i32,
            score: score + if lower.starts_with(query) { 1.0 } else { 0.0 },
            personal: false,
        });
    }
}

fn deduplicate_suggestions(items: &mut Vec<pb::Suggestion>) {
    let mut best = HashMap::<String, pb::Suggestion>::new();
    for item in items.drain(..) {
        let key = item.text.to_lowercase();
        match best.get(&key) {
            Some(existing) if existing.score >= item.score => {}
            _ => {
                best.insert(key, item);
            }
        }
    }
    items.extend(best.into_values());
}

fn search_results(
    contents: &[bbs_link_pb::Content],
    query: &str,
    search_type: pb::SearchType,
    excluded_authors: &HashSet<String>,
    source_ranked: bool,
    route_context: Option<&RouteSearchContext>,
    semantic: bool,
) -> Vec<pb::SearchResult> {
    let visible_contents = contents
        .iter()
        .filter(|content| !excluded_authors.contains(&content.author_id))
        .filter(|content| {
            route_context.is_none_or(|context| route_matches_context(content, context))
        })
        .collect::<Vec<_>>();
    match search_type {
        pb::SearchType::Posts => {
            content_results(&visible_contents, query, true, false, source_ranked)
        }
        pb::SearchType::Journeys => {
            content_results(&visible_contents, query, false, true, source_ranked)
        }
        pb::SearchType::Users => user_results(&visible_contents, query),
        pb::SearchType::Topics => topic_results(&visible_contents, query),
        pb::SearchType::Nodes => action_node_results(&visible_contents, query, semantic),
        pb::SearchType::Equipment => scene_equipment_results(&visible_contents, query, semantic),
        pb::SearchType::Resources => Vec::new(),
        pb::SearchType::All => {
            let mut results = content_results(&visible_contents, query, true, true, source_ranked);
            results.extend(user_results(&visible_contents, query));
            results.extend(topic_results(&visible_contents, query));
            results
        }
    }
}

// Entity results are extracted domain-side for BOTH pipelines: indexed hits
// revalidate into summaries that rebuild `route_template.actions`, and the
// BBS Link fallback reads full templates — so nodes and gear keep identical
// typed semantics no matter which source served the candidates. The semantic
// lane (`semantic=true`) recalls whole routes by vector distance, so every
// attached node/gear is a candidate and the caller's reranker decides order.
fn action_node_results(
    contents: &[&bbs_link_pb::Content],
    query: &str,
    semantic: bool,
) -> Vec<pb::SearchResult> {
    contents
        .iter()
        .filter(|content| content.content_type == bbs_link_pb::ContentType::Route as i32)
        .filter_map(|content| {
            Some((content, content.post.as_ref()?, content.route_template.as_ref()?))
        })
        .flat_map(|(content, post, template)| {
            template
                .actions
                .iter()
                .filter_map(|action| {
                    let metadata = format!(
                        "{} {} {}",
                        post.title,
                        post.summary,
                        action.scene_equipment.join(" ")
                    );
                    // An exact node-id hit is a legitimate structured lookup.
                    let id_hit = !query.is_empty() && action.id == query;
                    let (mut score, mut highlights) = if id_hit {
                        (10.0, vec![action.title.clone()])
                    } else if semantic {
                        (2.0, Vec::new())
                    } else {
                        relevance(query, &[action.title.as_str(), action.detail.as_str()], &metadata)?
                    };
                    score += content.quality_score;
                    highlights.retain(|value| !value.trim().is_empty());
                    Some(pb::SearchResult {
                        id: action.id.clone(),
                        result_type: pb::SearchResultType::ActionNode as i32,
                        title: action.title.clone(),
                        snippet: if action.detail.is_empty() {
                            post.summary.clone()
                        } else {
                            action.detail.clone()
                        },
                        cover_url: non_empty(&post.cover_url),
                        author_id: Some(content.author_id.clone()),
                        author_name: Some(post.author_name.clone()),
                        domain: Some(growth_domain(post.domain)),
                        score,
                        highlights,
                        // The enclosing route card travels with every node so
                        // scene-aware commerce and wayfinding stay attached.
                        post: Some(post_summary(post.clone(), template.actions.clone())),
                        resource: None,
                        ad: None,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn scene_equipment_results(
    contents: &[&bbs_link_pb::Content],
    query: &str,
    semantic: bool,
) -> Vec<pb::SearchResult> {
    let needle = query.to_lowercase();
    contents
        .iter()
        .filter(|content| content.content_type == bbs_link_pb::ContentType::Route as i32)
        .filter_map(|content| {
            Some((content, content.post.as_ref()?, content.route_template.as_ref()?))
        })
        .flat_map(|(content, post, template)| {
            let mut matches = Vec::new();
            for action in &template.actions {
                for gear in &action.scene_equipment {
                    if !semantic && !gear.to_lowercase().contains(&needle) {
                        continue;
                    }
                    // Equipment has no standalone record; its identity is the
                    // route node it belongs to. Same-gear hits across routes
                    // deliberately stay separate so route attribution is kept.
                    let exact = gear.to_lowercase() == needle;
                    let score = if semantic {
                        2.0
                    } else if exact {
                        8.0
                    } else {
                        4.0
                    } + content.quality_score;
                    matches.push(pb::SearchResult {
                        id: format!("{}/{}/equipment/{}", content.id, action.id, gear),
                        result_type: pb::SearchResultType::SceneEquipment as i32,
                        title: gear.clone(),
                        snippet: format!("{} · {}", post.title, action.title),
                        cover_url: non_empty(&post.cover_url),
                        author_id: Some(content.author_id.clone()),
                        author_name: Some(post.author_name.clone()),
                        domain: Some(growth_domain(post.domain)),
                        score,
                        highlights: Vec::new(),
                        post: Some(post_summary(post.clone(), template.actions.clone())),
                        resource: None,
                        ad: None,
                    });
                }
            }
            matches
        })
        .collect()
}

fn sort_results(items: &mut [pb::SearchResult]) {
    items.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn result_identity(item: &pb::SearchResult) -> String {
    let result_type = match pb::SearchResultType::try_from(item.result_type) {
        Ok(pb::SearchResultType::Post) => "post",
        Ok(pb::SearchResultType::Journey) => "journey",
        Ok(pb::SearchResultType::User) => "user",
        Ok(pb::SearchResultType::Topic) => "topic",
        Ok(pb::SearchResultType::Resource) => "resource",
        Ok(pb::SearchResultType::Ad) => "ad",
        Ok(pb::SearchResultType::ActionNode) => "node",
        Ok(pb::SearchResultType::SceneEquipment) => "equipment",
        Err(_) => "resource",
    };
    format!("{result_type}:{}", item.id)
}

fn content_results(
    contents: &[&bbs_link_pb::Content],
    query: &str,
    include_posts: bool,
    include_journeys: bool,
    source_ranked: bool,
) -> Vec<pb::SearchResult> {
    contents
        .iter()
        .filter(|content| {
            if content.content_type == bbs_link_pb::ContentType::Route as i32 {
                include_journeys
            } else {
                include_posts
            }
        })
        .filter_map(|content| {
            let post = content.post.as_ref()?;
            let metadata = format!(
                "{} {} {}",
                post.tags.join(" "),
                content.topics.join(" "),
                route_action_search_context(content),
            );
            let (mut score, highlights) = if source_ranked {
                // A revalidated OpenSearch hit may match its current body,
                // which is deliberately absent from the compact public read.
                relevance(
                    query,
                    &[post.title.as_str(), post.summary.as_str()],
                    &metadata,
                )
                .unwrap_or((0.0, Vec::new()))
            } else {
                relevance(
                    query,
                    &[
                        post.title.as_str(),
                        post.summary.as_str(),
                        content.body.as_str(),
                    ],
                    &metadata,
                )?
            };
            score += content.quality_score;
            Some(pb::SearchResult {
                id: content.id.clone(),
                result_type: if content.content_type == bbs_link_pb::ContentType::Route as i32 {
                    pb::SearchResultType::Journey as i32
                } else {
                    pb::SearchResultType::Post as i32
                },
                title: post.title.clone(),
                snippet: post.summary.clone(),
                cover_url: non_empty(&post.cover_url),
                author_id: Some(content.author_id.clone()),
                author_name: Some(post.author_name.clone()),
                domain: Some(growth_domain(post.domain)),
                score,
                highlights,
                post: Some(post_summary(
                    post.clone(),
                    content
                        .route_template
                        .as_ref()
                        .map(|template| template.actions.clone())
                        .unwrap_or_default(),
                )),
                resource: None,
                ad: None,
            })
        })
        .collect()
}

// Route actions are first-class search context. This mirrors the flattened
// OpenSearch document fields so fallback reads can find the same routes when
// OpenSearch is unavailable.
fn route_action_search_context(content: &bbs_link_pb::Content) -> String {
    content
        .route_template
        .iter()
        .flat_map(|template| &template.actions)
        .flat_map(|action| {
            std::iter::once(action.id.as_str())
                .chain(std::iter::once(action.title.as_str()))
                .chain(std::iter::once(action.detail.as_str()))
                .chain(std::iter::once(action.scheduled_label.as_str()))
                .chain(action.scene_equipment.iter().map(String::as_str))
        })
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn route_matches_context(content: &bbs_link_pb::Content, context: &RouteSearchContext) -> bool {
    content.id == context.route_id
        && route_template_matches_action(
            content.route_template.as_ref(),
            &context.action_node_id,
            &context.scene_equipment,
        )
}

fn route_matches_context_summary(
    summary: &bbs_link_pb::PublicContentSummary,
    context: &RouteSearchContext,
) -> bool {
    summary.id == context.route_id
        && summary.content_type == bbs_link_pb::ContentType::Route as i32
        && summary.route_actions.iter().any(|action| {
            action.id == context.action_node_id
                && (context.scene_equipment.is_empty()
                    || action.scene_equipment.iter().any(|equipment| {
                        equipment.trim().to_lowercase() == context.scene_equipment
                    }))
        })
}

fn route_template_matches_action(
    template: Option<&bbs_link_pb::RouteTemplate>,
    action_node_id: &str,
    scene_equipment: &str,
) -> bool {
    template.is_some_and(|template| {
        template.actions.iter().any(|action| {
            action.id == action_node_id
                && (scene_equipment.is_empty()
                    || action
                        .scene_equipment
                        .iter()
                        .any(|equipment| equipment.trim().to_lowercase() == scene_equipment))
        })
    })
}

fn user_results(contents: &[&bbs_link_pb::Content], query: &str) -> Vec<pb::SearchResult> {
    let mut authors = HashMap::<String, (&bbs_link_pb::PostSummary, usize, f64)>::new();
    for content in contents {
        let Some(post) = content.post.as_ref() else {
            continue;
        };
        let entry =
            authors
                .entry(content.author_id.clone())
                .or_insert((post, 0, content.quality_score));
        entry.1 += 1;
        entry.2 = entry.2.max(content.quality_score);
    }
    authors
        .into_iter()
        .filter_map(|(author_id, (post, content_count, quality))| {
            let (score, highlights) = relevance(query, &[post.author_name.as_str()], "")?;
            Some(pb::SearchResult {
                id: author_id.clone(),
                result_type: pb::SearchResultType::User as i32,
                title: post.author_name.clone(),
                snippet: format!("{content_count} 篇公开内容"),
                cover_url: non_empty(&post.author_avatar_url),
                author_id: Some(author_id),
                author_name: Some(post.author_name.clone()),
                domain: None,
                score: score + quality * 0.2,
                highlights,
                post: None,
                resource: None,
                ad: None,
            })
        })
        .collect()
}

fn topic_results(contents: &[&bbs_link_pb::Content], query: &str) -> Vec<pb::SearchResult> {
    let mut topics = HashMap::new();
    for content in contents {
        let Some(post) = content.post.as_ref() else {
            continue;
        };
        let content_topics: HashSet<_> = post.tags.iter().chain(&content.topics).collect();
        for topic in content_topics {
            let entry = topics.entry(topic.clone()).or_insert((
                0_usize,
                content.quality_score,
                post.domain,
            ));
            entry.0 += 1;
            entry.1 = entry.1.max(content.quality_score);
        }
    }
    topics
        .into_iter()
        .filter_map(|(topic, (content_count, quality, domain))| {
            let (score, highlights) = relevance(query, &[topic.as_str()], "")?;
            Some(pb::SearchResult {
                id: format!("topic:{topic}"),
                result_type: pb::SearchResultType::Topic as i32,
                title: topic,
                snippet: format!("{content_count} 条相关内容"),
                cover_url: None,
                author_id: None,
                author_name: None,
                domain: Some(growth_domain(domain)),
                score: score + quality * 0.1,
                highlights,
                post: None,
                resource: None,
                ad: None,
            })
        })
        .collect()
}

fn relevance(query: &str, primary_fields: &[&str], metadata: &str) -> Option<(f64, Vec<String>)> {
    let query = query.to_lowercase();
    let mut score = 0.0;
    let mut highlights = Vec::new();
    for (index, field) in primary_fields.iter().enumerate() {
        if field.to_lowercase().contains(&query) {
            score += if index == 0 { 7.0 } else { 3.0 };
            highlights.push((*field).to_string());
        }
    }
    if metadata.to_lowercase().contains(&query) {
        score += 2.0;
    }
    let haystack = primary_fields.join(" ") + " " + metadata;
    let term_hits = query
        .split_whitespace()
        .filter(|term| haystack.to_lowercase().contains(term))
        .count();
    score += term_hits as f64;
    (score > 0.0).then_some((score, highlights))
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn post_summary(
    value: bbs_link_pb::PostSummary,
    route_actions: Vec<bbs_link_pb::RouteTemplateAction>,
) -> pb::PostSummary {
    pb::PostSummary {
        id: value.id,
        author_name: value.author_name,
        author_avatar_url: value.author_avatar_url,
        title: value.title,
        summary: value.summary,
        domain: growth_domain(value.domain),
        cover_url: value.cover_url,
        route_title: value.route_title,
        route_duration: value.route_duration,
        join_count: value.join_count,
        like_count: value.like_count,
        freshness: value.freshness,
        tags: value.tags,
        is_route: value.is_route,
        is_milestone: value.is_milestone,
        is_question: value.is_question,
        route_actions,
    }
}

fn growth_domain(value: i32) -> i32 {
    match bbs_link_pb::GrowthDomain::try_from(value) {
        Ok(bbs_link_pb::GrowthDomain::Learning) => pb::GrowthDomain::Learning as i32,
        Ok(bbs_link_pb::GrowthDomain::Movement) => pb::GrowthDomain::Movement as i32,
        Ok(bbs_link_pb::GrowthDomain::Wellness) => pb::GrowthDomain::Wellness as i32,
        Ok(bbs_link_pb::GrowthDomain::Travel) => pb::GrowthDomain::Travel as i32,
        Ok(bbs_link_pb::GrowthDomain::Leisure) | Err(_) => pb::GrowthDomain::Unspecified as i32,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::datasource::{
        MemorySearchAnalytics, MemorySearchSessionStore, SearchSessionStore, SearchSourceResult,
    };

    struct StaticSearchSource {
        items: Vec<bbs_link_pb::Content>,
        degraded: bool,
    }

    struct PagedSearchSource {
        items: Vec<bbs_link_pb::Content>,
    }

    #[async_trait]
    impl SearchSource for StaticSearchSource {
        async fn contents(
            &self,
            _query: bbs_link_pb::ListRequest,
        ) -> Result<SearchSourceResult, SearchSourceError> {
            Ok(SearchSourceResult {
                page: bbs_link_pb::ContentPage {
                    items: self.items.clone(),
                    next_cursor: None,
                    total_estimate: self.items.len() as u64,
                },
                degraded: self.degraded,
                source_ranked: false,
            })
        }
    }

    #[async_trait]
    impl SearchSource for PagedSearchSource {
        async fn contents(
            &self,
            query: bbs_link_pb::ListRequest,
        ) -> Result<SearchSourceResult, SearchSourceError> {
            let offset = query
                .cursor
                .as_deref()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let limit = query.limit.unwrap_or(100);
            let items = self
                .items
                .iter()
                .skip(offset)
                .take(limit as usize)
                .cloned()
                .collect::<Vec<_>>();
            let next_offset = offset + items.len();
            Ok(SearchSourceResult {
                page: bbs_link_pb::ContentPage {
                    items,
                    next_cursor: (next_offset < self.items.len()).then(|| next_offset.to_string()),
                    total_estimate: self.items.len() as u64,
                },
                degraded: false,
                source_ranked: false,
            })
        }
    }

    #[test]
    fn revalidation_drops_stale_hits_and_rebuilds_search_fields() {
        let mut first = content("post-1", "索引中的旧作者", "索引中的旧标题", "旧话题");
        first.author_id = "stale-author".to_string();
        first.content_type = bbs_link_pb::ContentType::Article as i32;
        first.status = bbs_link_pb::ContentStatus::Restricted as i32;
        first.body = "已删除的旧正文命中".to_string();
        first.media = vec![bbs_link_pb::ContentMedia {
            id: "stale-media".to_string(),
            url: "https://stale.example/media".to_string(),
            kind: "image".to_string(),
            width: 1,
            height: 1,
            duration_ms: None,
        }];
        let second = content("post-2", "索引中的第二作者", "索引中的第二标题", "旧话题");
        let stale = content("post-3", "已受限作者", "已受限标题", "旧话题");
        let (page, stale_index_hits) = reconcile_indexed_page(
            bbs_link_pb::ContentPage {
                items: vec![first, second, stale],
                next_cursor: Some("private-pit-cursor".to_string()),
                total_estimate: 3,
            },
            bbs_link_pb::PublicContentSummaries {
                items: vec![
                    public_summary(
                        "post-2",
                        "权威第二作者",
                        "权威第二标题",
                        "权威第二摘要",
                        "第二话题",
                        bbs_link_pb::ContentType::Article,
                        0.4,
                    ),
                    public_summary(
                        "post-1",
                        "权威作者",
                        "权威标题",
                        "权威摘要",
                        "权威话题",
                        bbs_link_pb::ContentType::Route,
                        0.9,
                    ),
                ],
            },
            None,
        )
        .expect("valid authoritative summaries");

        assert!(stale_index_hits);
        assert_eq!(page.next_cursor.as_deref(), Some("private-pit-cursor"));
        assert_eq!(page.total_estimate, 3);
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["post-1", "post-2"],
            "the OpenSearch rank order is retained"
        );
        let first = &page.items[0];
        assert_eq!(first.author_id, "author-post-1");
        assert_eq!(first.content_type, bbs_link_pb::ContentType::Route as i32);
        assert_eq!(first.status, bbs_link_pb::ContentStatus::Published as i32);
        assert_eq!(first.topics, vec!["权威话题"]);
        assert_eq!(first.quality_score, 0.9);
        assert_eq!(
            first.post.as_ref().map(|post| post.title.as_str()),
            Some("权威标题")
        );
        assert!(
            first.body.is_empty(),
            "stale indexed body is never retained"
        );
        assert!(
            first.media.is_empty(),
            "stale indexed media is never retained"
        );
        assert!(first.created_at.is_empty());
        assert!(first.published_at.is_none());
        assert!(first.route_template.is_none());
    }

    #[test]
    fn revalidation_rejects_malformed_authoritative_summary_batches() {
        let indexed = bbs_link_pb::ContentPage {
            items: vec![content("post-1", "索引作者", "索引标题", "话题")],
            ..Default::default()
        };
        let mut mismatched = public_summary(
            "post-1",
            "权威作者",
            "权威标题",
            "权威摘要",
            "权威话题",
            bbs_link_pb::ContentType::Article,
            0.5,
        );
        mismatched.post.as_mut().expect("post summary").id = "other-id".to_string();
        assert!(
            reconcile_indexed_page(
                indexed.clone(),
                bbs_link_pb::PublicContentSummaries {
                    items: vec![mismatched]
                },
                None,
            )
            .is_err()
        );

        let duplicate = public_summary(
            "post-1",
            "权威作者",
            "权威标题",
            "权威摘要",
            "权威话题",
            bbs_link_pb::ContentType::Article,
            0.5,
        );
        assert!(
            reconcile_indexed_page(
                indexed.clone(),
                bbs_link_pb::PublicContentSummaries {
                    items: vec![duplicate.clone(), duplicate]
                },
                None,
            )
            .is_err()
        );

        assert!(
            reconcile_indexed_page(
                indexed,
                bbs_link_pb::PublicContentSummaries {
                    items: vec![public_summary(
                        "unexpected-post",
                        "权威作者",
                        "权威标题",
                        "权威摘要",
                        "权威话题",
                        bbs_link_pb::ContentType::Article,
                        0.5,
                    )]
                },
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn revalidated_body_only_matches_keep_the_rank_without_exposing_body() {
        let content = bbs_link_pb::Content {
            id: "post-1".to_string(),
            post: Some(bbs_link_pb::PostSummary {
                id: "post-1".to_string(),
                author_name: "权威作者".to_string(),
                title: "当前标题".to_string(),
                summary: "当前摘要".to_string(),
                ..Default::default()
            }),
            author_id: "author-post-1".to_string(),
            content_type: bbs_link_pb::ContentType::Article as i32,
            status: bbs_link_pb::ContentStatus::Published as i32,
            quality_score: 0.8,
            ..Default::default()
        };

        let results = content_results(&[&content], "旧正文词", true, false, true);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "当前标题");
        assert!(results[0].highlights.is_empty());
    }

    fn route_with_entities() -> bbs_link_pb::Content {
        let mut route = content("route-1", "路线作者", "周末徒步路线", "徒步");
        route.content_type = bbs_link_pb::ContentType::Route as i32;
        route.post.as_mut().expect("post").is_route = true;
        route.quality_score = 0.9;
        route.route_template = Some(bbs_link_pb::RouteTemplate {
            actions: vec![
                bbs_link_pb::RouteTemplateAction {
                    id: "node-summit".to_string(),
                    title: "登顶观景".to_string(),
                    detail: "在山顶完成 10 分钟正念".to_string(),
                    scene_equipment: vec!["登山鞋".to_string(), "登山杖".to_string()],
                    ..Default::default()
                },
                bbs_link_pb::RouteTemplateAction {
                    id: "node-journal".to_string(),
                    title: "复盘记录".to_string(),
                    detail: String::new(),
                    scene_equipment: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        route
    }

    #[test]
    fn action_node_search_returns_typed_nodes_carrying_the_route_card() {
        let route = route_with_entities();

        let results = action_node_results(&[&route], "登顶", false);

        assert_eq!(results.len(), 1);
        let node = &results[0];
        assert_eq!(node.id, "node-summit");
        assert_eq!(node.result_type, pb::SearchResultType::ActionNode as i32);
        assert_eq!(node.title, "登顶观景");
        let card = node.post.as_ref().expect("enclosing route card");
        assert_eq!(card.id, "route-1");
        assert_eq!(card.route_actions.len(), 2, "full public route context travels along");

        assert!(
            action_node_results(&[&route], "不存在的节点词", false).is_empty(),
            "only entity matches become typed nodes"
        );
    }

    #[test]
    fn scene_equipment_search_keeps_route_and_node_attribution() {
        let route = route_with_entities();

        let results = scene_equipment_results(&[&route], "登山鞋", false);

        assert_eq!(results.len(), 1);
        let gear = &results[0];
        assert_eq!(gear.result_type, pb::SearchResultType::SceneEquipment as i32);
        assert_eq!(
            gear.id,
            "route-1/node-summit/equipment/登山鞋",
            "identity binds the gear to its route and action"
        );
        let exact = results[0].score;
        // An exact term outranks a partial mention on the same quality score.
        let partial = scene_equipment_results(&[&route], "登山", false);
        assert!(partial.iter().all(|item| item.score < exact));
    }

    #[test]
    fn semantic_extraction_keeps_every_entity_without_a_lexical_match() {
        // The semantic lane recalls whole routes by vector distance, so a
        // paraphrased query must still surface the attached typed entities.
        let route = route_with_entities();

        let nodes = action_node_results(&[&route], "野外正念", true);
        assert_eq!(nodes.len(), 2);
        assert!(nodes.iter().all(|item| item.result_type == pb::SearchResultType::ActionNode as i32));
        assert_eq!(nodes[0].id, "node-summit", "k-NN document order is retained");
        assert!(nodes.iter().all(|item| item.score > 0.0));

        let gear = scene_equipment_results(&[&route], "野外露营", true);
        assert_eq!(gear.len(), 2);
        assert_eq!(
            gear.iter().map(|item| item.title.as_str()).collect::<Vec<_>>(),
            vec!["登山鞋", "登山杖"]
        );
        assert!(gear.iter().all(|item| item.result_type == pb::SearchResultType::SceneEquipment as i32));

        // The lexical lanes keep their match gate.
        assert!(action_node_results(&[&route], "野外正念", false).is_empty());
        assert!(scene_equipment_results(&[&route], "野外露营", false).is_empty());
    }

    #[tokio::test]
    async fn equipment_search_end_to_end_stays_typed_through_the_pipeline() {
        let route = route_with_entities();
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![route],
            degraded: false,
        }));

        let response = service
            .search(request("登山鞋", pb::SearchType::Equipment, None, Some(5)))
            .await
            .expect("equipment search succeeds");

        assert_eq!(response.items.len(), 1);
        assert_eq!(
            response.items[0].result_type,
            pb::SearchResultType::SceneEquipment as i32
        );
    }

    #[test]
    fn revalidation_preserves_public_route_actions_without_private_fields() {
        let indexed = bbs_link_pb::ContentPage {
            items: vec![content("route-1", "索引作者", "索引路线", "路线")],
            next_cursor: Some("pit".to_string()),
            total_estimate: 1,
        };
        let mut summary = public_summary(
            "route-1",
            "权威作者",
            "权威路线",
            "权威摘要",
            "路线",
            bbs_link_pb::ContentType::Route,
            0.8,
        );
        summary.route_actions = vec![bbs_link_pb::RouteTemplateAction {
            id: "node-1".to_string(),
            title: "带装备行动".to_string(),
            scene_equipment: vec!["登山鞋".to_string()],
            ..Default::default()
        }];

        let (page, degraded) = reconcile_indexed_page(
            indexed,
            bbs_link_pb::PublicContentSummaries {
                items: vec![summary],
            },
            None,
        )
        .expect("public route summary should reconcile");

        assert!(!degraded);
        let route = page.items.first().expect("route result");
        let template = route.route_template.as_ref().expect("public actions");
        assert_eq!(template.actions[0].id, "node-1");
        assert_eq!(template.actions[0].scene_equipment, vec!["登山鞋"]);
        assert!(route.body.is_empty());
        assert!(route.media.is_empty());
    }

    #[tokio::test]
    async fn rejects_oversized_route_context_fields() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: Vec::new(),
            degraded: false,
        }));
        let mut request = request("路线", pb::SearchType::Journeys, None, None);
        request.route_id = Some("r".repeat(MAX_ROUTE_CONTEXT_FIELD_LENGTH + 1));
        request.action_node_id = Some("node-1".to_string());
        let error = service
            .search(request)
            .await
            .expect_err("oversized route context must be rejected");
        assert!(matches!(error, SearchError::Validation(_)));
    }

    #[tokio::test]
    async fn searches_users_and_topics() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![content("post-1", "一册", "阅读", "主题阅读")],
            degraded: false,
        }));
        let users = service
            .search(request("一册", pb::SearchType::Users, None, None))
            .await
            .expect("user search");
        let topics = service
            .search(request("主题", pb::SearchType::Topics, None, None))
            .await
            .expect("topic search");

        assert_eq!(
            users.items[0].result_type,
            pb::SearchResultType::User as i32
        );
        assert_eq!(
            topics.items[0].result_type,
            pb::SearchResultType::Topic as i32
        );
        assert_eq!(topics.items[0].snippet, "1 条相关内容");
    }

    #[tokio::test]
    async fn paginates_ranked_results() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读方法一", "阅读"),
                content("post-2", "二页", "阅读方法二", "阅读"),
            ],
            degraded: false,
        }));
        let first = service
            .search(request("阅读", pb::SearchType::Posts, None, Some(1)))
            .await
            .expect("first page");
        let second = service
            .search(request(
                "阅读",
                pb::SearchType::Posts,
                first.next_cursor.clone(),
                Some(1),
            ))
            .await
            .expect("second page");

        assert_eq!(first.items.len(), 1);
        assert!(
            first
                .next_cursor
                .as_deref()
                .is_some_and(|value| value.starts_with("v3-"))
        );
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].id, second.items[0].id);
    }

    #[tokio::test]
    async fn rejects_a_cursor_from_a_different_query() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读方法一", "阅读"),
                content("post-2", "二页", "阅读方法二", "阅读"),
            ],
            degraded: false,
        }));
        let first = service
            .search(request("阅读", pb::SearchType::Posts, None, Some(1)))
            .await
            .expect("first page");

        let error = service
            .search(request(
                "跑步",
                pb::SearchType::Posts,
                first.next_cursor,
                Some(1),
            ))
            .await
            .expect_err("cursor must be bound to its query");

        assert!(matches!(error, SearchError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_a_cursor_from_a_different_result_type() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读方法一", "阅读"),
                content("post-2", "二页", "阅读方法二", "阅读"),
            ],
            degraded: false,
        }));
        let first = service
            .search(request("阅读", pb::SearchType::Posts, None, Some(1)))
            .await
            .expect("first page");

        let error = service
            .search(request(
                "阅读",
                pb::SearchType::Users,
                first.next_cursor,
                Some(1),
            ))
            .await
            .expect_err("cursor must be bound to its result type");

        assert!(matches!(error, SearchError::Validation(_)));
    }

    #[tokio::test]
    async fn continues_beyond_the_first_source_page_without_duplicates() {
        let items = (0..250)
            .map(|index| {
                content(
                    &format!("post-{index:03}"),
                    "一册",
                    &format!("阅读方法 {index}"),
                    "阅读",
                )
            })
            .collect();
        let service = SearchService::new(Arc::new(PagedSearchSource { items }));
        let mut cursor = None;
        let mut ids = HashSet::new();
        let mut pages = 0;
        loop {
            let response = service
                .search(request("阅读", pb::SearchType::Posts, cursor, Some(50)))
                .await
                .expect("page search");
            pages += 1;
            for item in response.items {
                assert!(ids.insert(item.id), "result must not be repeated");
            }
            cursor = response.next_cursor;
            if cursor.is_none() {
                break;
            }
        }

        assert_eq!(pages, 5);
        assert_eq!(ids.len(), 250);
    }

    #[tokio::test]
    async fn fills_a_page_after_excluding_an_entire_source_page() {
        let mut items = (0..100)
            .map(|index| {
                let mut item = content(
                    &format!("hidden-{index:03}"),
                    "不可见作者",
                    &format!("阅读方法 {index}"),
                    "阅读",
                );
                item.author_id = "author-hidden".to_string();
                item
            })
            .collect::<Vec<_>>();
        items.extend([
            content("visible-1", "可见作者", "阅读方法一", "阅读"),
            content("visible-2", "可见作者", "阅读方法二", "阅读"),
        ]);
        let service = SearchService::new(Arc::new(PagedSearchSource { items }));

        let response = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Posts,
                None,
                Some(2),
                "viewer-a",
                &["author-hidden"],
            ))
            .await
            .expect("visible page search");

        assert_eq!(response.items.len(), 2);
        assert!(
            response
                .items
                .iter()
                .all(|item| item.author_id.as_deref() != Some("author-hidden"))
        );
        assert!(response.next_cursor.is_none());
        assert_eq!(response.total_estimate, 2);
    }

    #[tokio::test]
    async fn excludes_authors_before_building_user_and_topic_results() {
        let mut hidden = content("hidden-1", "阅读隐藏", "阅读笔记", "阅读");
        hidden.author_id = "author-hidden".to_string();
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                hidden,
                content("visible-1", "阅读可见一", "阅读笔记一", "阅读"),
                content("visible-2", "阅读可见二", "阅读笔记二", "阅读"),
            ],
            degraded: false,
        }));

        let users = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Users,
                None,
                None,
                "viewer-a",
                &["author-hidden"],
            ))
            .await
            .expect("user search");
        let topics = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Topics,
                None,
                None,
                "viewer-a",
                &["author-hidden"],
            ))
            .await
            .expect("topic search");

        assert!(
            users
                .items
                .iter()
                .all(|item| item.author_id.as_deref() != Some("author-hidden"))
        );
        assert_eq!(topics.items.len(), 1);
        assert_eq!(topics.items[0].snippet, "2 条相关内容");
    }

    #[tokio::test]
    async fn excludes_authors_from_content_derived_suggestions() {
        let mut hidden = content("hidden-1", "专属屏蔽", "屏蔽专属", "专属话题");
        hidden.author_id = "author-hidden".to_string();
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                hidden,
                content("visible-1", "专属可见", "可见专属", "专属话题"),
            ],
            degraded: false,
        }));

        let suggestions = service
            .suggestions(pb::SuggestionsRequest {
                q: "专属".to_string(),
                user_id: Some("viewer-a".to_string()),
                excluded_author_ids: vec!["author-hidden".to_string()],
            })
            .await
            .expect("suggestions");
        let texts = suggestions
            .items
            .into_iter()
            .map(|item| item.text)
            .collect::<HashSet<_>>();

        assert!(texts.contains("可见专属"));
        assert!(!texts.contains("屏蔽专属"));
        assert!(!texts.contains("专属屏蔽"));
    }

    #[tokio::test]
    async fn rejects_a_cursor_after_the_viewer_visibility_changes() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读方法一", "阅读"),
                content("post-2", "二页", "阅读方法二", "阅读"),
            ],
            degraded: false,
        }));
        let first = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Posts,
                None,
                Some(1),
                "viewer-a",
                &[],
            ))
            .await
            .expect("first search");

        let error = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Posts,
                first.next_cursor,
                Some(1),
                "viewer-a",
                &["author-post-1"],
            ))
            .await
            .expect_err("visibility changes must invalidate the cursor");

        assert!(matches!(error, SearchError::Validation(_)));
    }

    #[tokio::test]
    async fn accepts_reordered_visibility_exclusions_for_the_same_cursor() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读方法一", "阅读"),
                content("post-2", "二页", "阅读方法二", "阅读"),
            ],
            degraded: false,
        }));
        let first = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Posts,
                None,
                Some(1),
                "viewer-a",
                &["author-a", "author-b"],
            ))
            .await
            .expect("first search");

        let second = service
            .search(request_with_visibility(
                "阅读",
                pb::SearchType::Posts,
                first.next_cursor,
                Some(1),
                "viewer-a",
                &["author-b", "author-a"],
            ))
            .await
            .expect("canonical visibility policy should retain the cursor");

        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].id, second.items[0].id);
    }

    #[tokio::test]
    async fn reports_an_expired_server_side_session() {
        let sessions = Arc::new(MemorySearchSessionStore::default());
        let service = SearchService::with_dependencies(
            Some(Arc::new(StaticSearchSource {
                items: vec![
                    content("post-1", "一册", "阅读方法一", "阅读"),
                    content("post-2", "二页", "阅读方法二", "阅读"),
                ],
                degraded: false,
            })),
            Arc::new(MemorySearchAnalytics::default()),
            sessions.clone(),
        );
        let first = service
            .search(request("阅读", pb::SearchType::Posts, None, Some(1)))
            .await
            .expect("first page");
        let cursor = first.next_cursor.expect("continuation cursor");
        let id = cursor
            .strip_prefix("v3-")
            .and_then(|value| value.split_once('-'))
            .map(|(_, id)| id)
            .expect("session id");
        sessions.delete(id).await.expect("delete session");

        let error = service
            .search(request(
                "阅读",
                pb::SearchType::Posts,
                Some(cursor),
                Some(1),
            ))
            .await
            .expect_err("expired session");
        assert!(matches!(error, SearchError::CursorExpired));
    }

    #[tokio::test]
    async fn propagates_source_degradation() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![content("post-1", "一册", "阅读", "主题阅读")],
            degraded: true,
        }));

        let response = service
            .search(request("阅读", pb::SearchType::Posts, None, None))
            .await
            .expect("degraded search");

        assert!(response.degraded);
        assert_eq!(response.items.len(), 1);
    }

    #[tokio::test]
    async fn separates_posts_and_journeys_without_misclassifying_all_results() {
        let mut route = content("route-1", "领路人", "阅读入门路线", "阅读");
        route.content_type = bbs_link_pb::ContentType::Route as i32;
        let mut milestone = content("milestone-1", "同行者", "第一周阅读成果", "阅读");
        milestone.content_type = bbs_link_pb::ContentType::Milestone as i32;
        milestone
            .post
            .as_mut()
            .expect("milestone summary")
            .is_milestone = true;
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![
                content("post-1", "一册", "阅读笔记", "阅读"),
                milestone,
                route,
            ],
            degraded: false,
        }));

        let posts = service
            .search(request("阅读", pb::SearchType::Posts, None, None))
            .await
            .expect("post search");
        let all = service
            .search(request("阅读", pb::SearchType::All, None, None))
            .await
            .expect("all search");

        assert_eq!(posts.items.len(), 2);
        assert!(posts.items.iter().any(|item| item.id == "post-1"));
        assert!(posts.items.iter().any(|item| {
            item.id == "milestone-1"
                && item.result_type == pb::SearchResultType::Post as i32
                && item.post.as_ref().is_some_and(|post| post.is_milestone)
        }));
        assert!(all.items.iter().any(|item| {
            item.id == "route-1" && item.result_type == pb::SearchResultType::Journey as i32
        }));
    }

    #[tokio::test]
    async fn fallback_search_matches_route_action_nodes_and_equipment() {
        let mut route = content("route-1", "领路人", "力量入门路线", "训练");
        route.content_type = bbs_link_pb::ContentType::Route as i32;
        route.route_template = Some(bbs_link_pb::RouteTemplate {
            actions: vec![bbs_link_pb::RouteTemplateAction {
                id: "action-kettlebell".to_string(),
                title: "壶铃硬拉".to_string(),
                detail: "用壶铃完成基础髋铰链".to_string(),
                scheduled_label: "周二".to_string(),
                scene_equipment: vec!["壶铃".to_string(), "瑜伽垫".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        });
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![route],
            degraded: false,
        }));

        for query in ["壶铃", "action-kettlebell"] {
            let response = service
                .search(request(query, pb::SearchType::Journeys, None, None))
                .await
                .expect("route action context should be searchable");
            assert_eq!(
                response.items.first().map(|item| item.id.as_str()),
                Some("route-1"),
                "{query} should resolve the owning route"
            );
            let actions = response.items[0]
                .post
                .as_ref()
                .expect("route result should carry its public action nodes")
                .route_actions
                .as_slice();
            assert_eq!(actions[0].id, "action-kettlebell");
            assert!(actions[0].scene_equipment.iter().any(|item| item == "壶铃"));
        }
    }

    #[tokio::test]
    async fn structured_route_context_excludes_text_only_route_matches() {
        let mut route = content("route-1", "领路人", "壶铃训练路线", "训练");
        route.content_type = bbs_link_pb::ContentType::Route as i32;
        route.route_template = Some(bbs_link_pb::RouteTemplate {
            actions: vec![bbs_link_pb::RouteTemplateAction {
                id: "action-kettlebell".to_string(),
                title: "壶铃硬拉".to_string(),
                scene_equipment: vec!["壶铃".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        });
        let mut request = request("壶铃", pb::SearchType::Journeys, None, None);
        request.route_id = Some("route-1".to_string());
        request.action_node_id = Some("action-kettlebell".to_string());
        request.scene_equipment = Some("瑜伽垫".to_string());
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![route],
            degraded: false,
        }));
        let response = service.search(request).await.expect("context search");
        assert!(response.items.is_empty());
    }

    fn request(
        query: &str,
        search_type: pb::SearchType,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> pb::SearchRequest {
        pb::SearchRequest {
            q: query.to_string(),
            search_type: search_type as i32,
            cursor,
            limit: limit.map(|value| u32::try_from(value).unwrap_or(u32::MAX)),
            user_id: None,
            excluded_author_ids: Vec::new(),
            session_id: None,
            route_id: None,
            action_node_id: None,
            scene_equipment: None,
            ad_placement: None,
            geo_region: None,
            device_os: None,
        }
    }

    fn request_with_visibility(
        query: &str,
        search_type: pb::SearchType,
        cursor: Option<String>,
        limit: Option<usize>,
        viewer_id: &str,
        excluded_author_ids: &[&str],
    ) -> pb::SearchRequest {
        pb::SearchRequest {
            user_id: Some(viewer_id.to_string()),
            excluded_author_ids: excluded_author_ids
                .iter()
                .map(|author_id| (*author_id).to_string())
                .collect(),
            ..request(query, search_type, cursor, limit)
        }
    }

    fn content(id: &str, author: &str, title: &str, topic: &str) -> bbs_link_pb::Content {
        bbs_link_pb::Content {
            id: id.to_string(),
            post: Some(bbs_link_pb::PostSummary {
                id: id.to_string(),
                author_name: author.to_string(),
                author_avatar_url: String::new(),
                title: title.to_string(),
                summary: "把方法用到行动中".to_string(),
                domain: bbs_link_pb::GrowthDomain::Learning as i32,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 1.0,
                tags: vec![topic.to_string()],
                is_route: false,
                is_milestone: false,
                is_question: false,
                fork_count: 0,
            }),
            author_id: format!("author-{id}"),
            content_type: bbs_link_pb::ContentType::Article as i32,
            status: bbs_link_pb::ContentStatus::Published as i32,
            body: "正文".to_string(),
            media: Vec::<bbs_link_pb::ContentMedia>::new(),
            topics: vec![topic.to_string()],
            created_at: "0".to_string(),
            published_at: Some("0".to_string()),
            question_context: None,
            version: 1,
            quality_score: 1.0,
            route_template: None,
            milestone: None,
            accepted_answer_id: None,
            route_fork: None,
        }
    }

    fn public_summary(
        id: &str,
        author_name: &str,
        title: &str,
        summary: &str,
        topic: &str,
        content_type: bbs_link_pb::ContentType,
        quality_score: f64,
    ) -> bbs_link_pb::PublicContentSummary {
        bbs_link_pb::PublicContentSummary {
            id: id.to_string(),
            post: Some(bbs_link_pb::PostSummary {
                id: id.to_string(),
                author_name: author_name.to_string(),
                title: title.to_string(),
                summary: summary.to_string(),
                tags: vec![topic.to_string()],
                is_route: content_type == bbs_link_pb::ContentType::Route,
                ..Default::default()
            }),
            author_id: format!("author-{id}"),
            content_type: content_type as i32,
            topics: vec![topic.to_string()],
            quality_score,
            route_actions: Vec::new(),
        }
    }
}
