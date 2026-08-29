use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use bookway_ad_main_api::pb::{self as ad_pb, ad_main_client::AdMainClient};
use bookway_bbs_api::pb::{self as bbs_participation_pb, bbs_client::BbsClient};
use bookway_bbs_link_api::pb::{self as bbs_link_pb, bbs_link_client::BbsLinkClient};
use bookway_bbs_search_api::pb::{self, bbs_search_client::BbsSearchClient};
use bookway_feature_main_api::pb::{self as feature_pb, feature_main_client::FeatureMainClient};
use bookway_knowledge_catalog_api::pb::{
    self as catalog_pb, knowledge_catalog_client::KnowledgeCatalogClient,
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    api::pb as api_pb,
    conf::Config,
    datasource::{
        MemoryQueryRewriteDao, MemorySearchExposureStore, MemorySearchSessionStore,
        PostgresQueryRewriteDao, PostgresSearchExposureStore, PostgresSearchSessionStore,
        QueryRewriteDictionary, RecallSource, RecallState, SearchAttribution, SearchExposure,
        SearchExposureError, SearchExposureItem, SearchPipelineSession, SearchSessionError,
        SearchSessionStore, SharedQueryRewriteDao, SharedSearchExposureStore,
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
const MAX_ROUTE_CONTEXT_FIELD_LENGTH: usize = 160;
const QUERY_REWRITE_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const BBS_SEARCH_TIMEOUT: Duration = Duration::from_millis(1_500);
const BBS_LINK_TIMEOUT: Duration = Duration::from_millis(1_500);
const KNOWLEDGE_CATALOG_TIMEOUT: Duration = Duration::from_millis(1_500);
/// Join counts are cosmetic social proof: one batched counts-only read that
/// must never crowd out ranking within the request budget.
const BBS_ROUTE_CONTEXT_TIMEOUT: Duration = Duration::from_millis(40);
/// Semantic recall is an additive lane; both its RPCs get tight budgets so a
/// slow embedding provider can never dominate the request.
const SEMANTIC_EMBED_TIMEOUT: Duration = Duration::from_millis(60);
const SEMANTIC_SEARCH_TIMEOUT: Duration = Duration::from_millis(60);
const FEATURE_RERANK_TIMEOUT: Duration = Duration::from_millis(35);
const MAX_FEATURE_RERANK_CANDIDATES: usize = 200;
const AD_DECISION_TIMEOUT: Duration = Duration::from_millis(25);
const DEFAULT_AD_PLACEMENT: &str = "search";
/// Search ads stay low density: 15% of the page, three organic results
/// minimum, slots from the shared `commercial-mix` schedule. The same policy
/// decides how many ads to request so supply never exceeds what a page can
/// legitimately render.
const SEARCH_AD_POLICY: bookway_commercial_mix::MixPolicy =
    bookway_commercial_mix::MixPolicy::new(1_500, 3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SearchIntent {
    Generic,
    Topic,
    User,
    Journey,
    Resource,
    /// Query names a route action node ("节点") — prefer typed node results.
    ActionNode,
    /// Query names scene gear ("装备"/"器材") — prefer typed equipment results.
    Equipment,
}

#[derive(Clone, Debug)]
struct RecallPlan {
    source: RecallSource,
    query: String,
}

#[derive(Clone, Debug)]
struct SearchPlan {
    original_query: String,
    recalls: Vec<RecallPlan>,
    intent: SearchIntent,
    /// The search type forwarded to BBS recalls. General surfaces route
    /// entity-intent queries to the typed NODES/EQUIPMENT indices while the
    /// caller-facing fingerprint keeps using the requested type.
    bbs_search_type: pb::SearchType,
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
    dao: SharedQueryRewriteDao,
    state: RwLock<Option<QueryRewriteCacheState>>,
    // Only one request refreshes the dictionary after expiry; concurrent
    // readers reuse the refreshed state instead of stampeding the database.
    refresh_lock: Mutex<()>,
}

impl QueryRewriteCache {
    fn new(dao: SharedQueryRewriteDao) -> Self {
        Self {
            dao,
            state: RwLock::new(None),
            refresh_lock: Mutex::new(()),
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
        let _refresh_guard = self.refresh_lock.lock().await;
        // Another request may have completed the refresh while this request
        // waited for the singleflight lock.
        {
            let state = self.state.read().await;
            if let Some(state) = state.as_ref()
                && state.refreshed_at.elapsed() < QUERY_REWRITE_REFRESH_INTERVAL
            {
                return QueryRewriteResolution {
                    dictionary: state.dictionary.clone(),
                    degraded: state.degraded,
                };
            }
        }
        match self.dao.active().await {
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
    #[error("knowledge-catalog resource request failed with {code:?}: {message}")]
    ResourceUpstream { code: tonic::Code, message: String },
    #[error("bbs-link returned an invalid public summary batch")]
    InvalidContentSummary,
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    search_client: BbsSearchClient<tonic::transport::Channel>,
    content_client: Option<BbsLinkClient<tonic::transport::Channel>>,
    bbs_client: Option<BbsClient<tonic::transport::Channel>>,
    resource_client: Option<KnowledgeCatalogClient<tonic::transport::Channel>>,
    feature_client: Option<FeatureMainClient<tonic::transport::Channel>>,
    ad_main: Option<AdMainClient<tonic::transport::Channel>>,
    sessions: Arc<dyn SearchSessionStore>,
    exposures: SharedSearchExposureStore,
    query_rewrites: Arc<QueryRewriteCache>,
}

impl Domain {
    pub(crate) async fn new(
        config: Config,
        search_client: BbsSearchClient<tonic::transport::Channel>,
        content_client: BbsLinkClient<tonic::transport::Channel>,
        bbs_client: Option<BbsClient<tonic::transport::Channel>>,
        resource_client: KnowledgeCatalogClient<tonic::transport::Channel>,
        feature_client: Option<FeatureMainClient<tonic::transport::Channel>>,
        ad_main: Option<AdMainClient<tonic::transport::Channel>>,
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
        let query_rewrites: SharedQueryRewriteDao = match pool {
            Some(pool) => Arc::new(PostgresQueryRewriteDao::new(pool)),
            None => Arc::new(MemoryQueryRewriteDao),
        };
        Ok(Self {
            config,
            search_client,
            content_client: Some(content_client),
            bbs_client,
            resource_client: Some(resource_client),
            feature_client,
            ad_main,
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
                bbs_url: String::new(),
                knowledge_catalog_url: String::new(),
                feature_main_url: String::new(),
                ad_main_url: String::new(),
            },
            search_client,
            // Unit tests supply already-authoritative candidates directly. Production
            // construction above always installs the BBS Link public-fact client.
            content_client: None,
            bbs_client: None,
            resource_client: None,
            feature_client: None,
            ad_main: None,
            sessions,
            exposures,
            query_rewrites: Arc::new(QueryRewriteCache::new(Arc::new(MemoryQueryRewriteDao))),
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
        let normalized_query = normalize_query(&request.q)?;
        request.q = normalized_query;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        request.limit = Some(limit as u32);
        request.excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
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
            return Err(SearchMainError::InvalidCursor(
                "route_id and action_node_id must be provided together".to_string(),
            ));
        }
        let route_context = route_context(&request);
        let plan = make_search_plan(
            &request.q,
            search_type,
            &rewrite_resolution.dictionary,
            self.resource_client.is_some() && route_context.is_none(),
        )?;
        request.q = plan.original_query.clone();
        let fingerprint = query_fingerprint(
            &plan.original_query,
            search_type,
            request.user_id.as_deref(),
            &request.excluded_author_ids,
            route_context.as_ref(),
        );
        let session_id = parse_cursor(request.cursor.as_deref(), fingerprint)?;
        let mut session = match session_id.as_deref() {
            Some(id) => self
                .sessions
                .load(id)
                .await?
                .filter(|session| session.query_fingerprint == fingerprint)
                .ok_or(SearchMainError::CursorExpired)?,
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

        self.hydrate_route_join_counts(&mut page, request.user_id.as_deref())
            .await;
        session.delivered_count += page.len();
        let mut ad_degraded = false;
        // Search ads are a first-page contextual mix. The exposure ledger below
        // filters ads out of its item rows, while retaining each organic result's
        // visual position, so ad delivery never becomes a fake search result in
        // attribution or pagination state. Displaced organic tail items go back
        // to the pending buffer head in order, so the next page resumes exactly
        // where this one's commercial slots cut it short.
        let ad_slots = SEARCH_AD_POLICY.ad_slots_for(limit);
        if request.cursor.is_none()
            && ad_slots > 0
            && page.len() >= SEARCH_AD_POLICY.min_natural_results
            && let Some(context) = route_context.as_ref()
            && !context.scene_equipment.is_empty()
            && let Some(user_id) = request
                .user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        {
            match self.contextual_search_ad(&request, context, user_id, ad_slots).await {
                Ok(ads) if !ads.is_empty() => {
                    let ads = ads.into_iter().map(|ad| pb::SearchResult {
                        id: format!("ad:{}", ad.campaign_id),
                        result_type: pb::SearchResultType::Ad as i32,
                        title: ad.title.clone(),
                        snippet: ad.body.clone(),
                        cover_url: (!ad.image_url.is_empty()).then(|| ad.image_url.clone()),
                        author_id: None,
                        author_name: None,
                        domain: None,
                        score: ad.ecpm,
                        highlights: Vec::new(),
                        post: None,
                        resource: None,
                        ad: Some(ad),
                    });
                    let organics = std::mem::take(&mut page);
                    let (mixed, overflow) =
                        bookway_commercial_mix::mix_page(organics, ads.collect(), limit, SEARCH_AD_POLICY);
                    page.reserve(mixed.len());
                    for item in mixed {
                        match item {
                            bookway_commercial_mix::MixedItem::Organic(result) => page.push(result),
                            bookway_commercial_mix::MixedItem::Ad(result) => page.push(result),
                        }
                    }
                    if !overflow.is_empty() {
                        let displaced = overflow.len();
                        // Recompute delivered_count so paging state reflects
                        // only what was actually rendered.
                        session.delivered_count = session.delivered_count.saturating_sub(displaced);
                        session.pending.splice(0..0, overflow);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    ad_degraded = true;
                    tracing::debug!(%error, "contextual search ad degraded");
                }
            }
        }
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
        // Anonymous searches still receive a request id for client tracing,
        // but never write a shared synthetic identity to the attribution
        // ledger. A common "anonymous" row would let unrelated visitors
        // collide on session/result checks and pollute training data.
        let exposure_degraded = if let Some(user_id) = request
            .user_id
            .as_deref()
            .map(str::trim)
            .filter(|user_id| !user_id.is_empty())
        {
            let tracking_session_id = request
                .session_id
                .clone()
                .filter(|session_id| !session_id.trim().is_empty())
                .unwrap_or_else(|| "anonymous-search-session".to_string());
            let exposure = SearchExposure {
                request_id: request_id.clone(),
                user_id: user_id.to_string(),
                session_id: tracking_session_id,
                query_hash: format!("{:016x}", stable_hash(&plan.original_query)),
                query_rewrite_version: session.query_rewrite_version.clone(),
                degraded: session.degraded,
                items: page
                    .iter()
                    .enumerate()
                    .filter(|(_, result)| result.ad.is_none())
                    .map(|(position, result)| SearchExposureItem {
                        position,
                        result_id: result.id.clone(),
                        result_type: pb::SearchResultType::try_from(result.result_type)
                            .map_or("unknown", |result_type| result_type.as_str_name())
                            .to_string(),
                    })
                    .collect(),
            };
            match self.exposures.record(exposure).await {
                Ok(()) => false,
                Err(error) => {
                    tracing::warn!(%error, request_id = %request_id, "search exposure persistence degraded");
                    true
                }
            }
        } else {
            false
        };
        tracing::debug!(
            query_hash = format_args!("{:016x}", stable_hash(&plan.original_query)),
            variants = session.recalls.len(),
            source_calls,
            candidates = session.delivered_count + session.pending.len(),
            took_ms = started.elapsed().as_millis() as u64,
            degraded = session.degraded || exposure_degraded || ad_degraded,
            "search pipeline completed"
        );
        Ok(pb::SearchResponse {
            query: plan.original_query,
            items: page,
            next_cursor,
            total_estimate: u64::try_from(total_estimate).unwrap_or(u64::MAX),
            took_ms: started.elapsed().as_millis() as u64,
            degraded: session.degraded || exposure_degraded || ad_degraded,
            request_id,
        })
    }

    async fn contextual_search_ad(
        &self,
        request: &pb::SearchRequest,
        context: &RouteSearchContext,
        user_id: &str,
        slots: usize,
    ) -> Result<Vec<pb::SearchAd>, String> {
        if slots == 0 {
            return Ok(Vec::new());
        }
        let Some(ad_main) = self.ad_main.as_ref() else {
            return Ok(Vec::new());
        };
        let placement = request
            .ad_placement
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_AD_PLACEMENT)
            .to_string();
        let rpc = ad_pb::DecisionRequest {
            user_id: user_id.to_string(),
            placement: placement.clone(),
            domain: None,
            limit: Some(u32::try_from(slots).unwrap_or(1)),
            route_id: context.route_id.clone(),
            action_node_id: context.action_node_id.clone(),
            scene_equipment: Some(context.scene_equipment.clone()),
            // Edge-derived delivery context; empty values fail closed to
            // unrestricted campaigns only (ad-center matching rule).
            geo_region: request.geo_region.clone().unwrap_or_default(),
            device_os: request.device_os.clone().unwrap_or_default(),
        };
        let mut client = ad_main.clone();
        let request = bookway_runtime::grpc_service_request(rpc)
            .map_err(|error| format!("ad-main request authentication failed: {error}"))?;
        let response = tokio::time::timeout(AD_DECISION_TIMEOUT, client.decide(request))
            .await
            .map_err(|_| "ad-main decision timed out".to_string())?
            .map_err(|error| error.to_string())?
            .into_inner();
        // Decision items arrive in auction (eCPM) order; keep that order so
        // the mixer consumes the strongest inventory first.
        Ok(response
            .items
            .into_iter()
            .filter(|ad| {
                ad.route_id == context.route_id
                    && ad.action_node_id == context.action_node_id
                    && ad.placement == placement
                    && !ad.campaign_id.trim().is_empty()
                    && !ad.request_id.trim().is_empty()
                    && ad.ecpm.is_finite()
                    && ad.ecpm >= 0.0
                    && ad
                        .scene_equipment
                        .trim()
                        .eq_ignore_ascii_case(&context.scene_equipment)
            })
            .map(|ad| {
                pb::SearchAd {
                request_id: ad.request_id,
                campaign_id: ad.campaign_id,
                placement: ad.placement,
                title: ad.title,
                body: ad.body,
                image_url: ad.image_url,
                landing_url: ad.landing_url,
                // `score` includes targeting and pacing tie-breakers and is
                // intentionally not a billable impression value. Preserve
                // the auction's normalized eCPM as the only pricing signal.
                ecpm: ad.ecpm,
                model_version: ad.model_version,
                route_id: ad.route_id,
                action_node_id: ad.action_node_id,
                scene_equipment: ad.scene_equipment,
            }
            })
            .collect())
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
        let mut fetched_candidates = Vec::new();
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
            if matches!(recall.source, RecallSource::Bbs) {
                source_request.search_type = plan.bbs_search_type as i32;
            }
            calls += 1;

            let recall_result = match recall.source {
                RecallSource::Bbs => self.search_bbs(source_request).await,
                RecallSource::Resource => self.search_resources(source_request).await,
                RecallSource::Semantic => {
                    // One-shot lane: the query vector is bound to this round's
                    // plan, so a continuation would only repeat candidates.
                    self.search_semantic_recall(request, plan).await
                }
            };
            match recall_result {
                Ok(response) => {
                    let recall = &mut session.recalls[index];
                    recall.source_cursor = response.next_cursor;
                    recall.exhausted = recall.source_cursor.is_none();
                    session.source_total_estimate = session
                        .source_total_estimate
                        .max(usize::try_from(response.total_estimate).unwrap_or(usize::MAX));
                    session.degraded |= response.degraded;
                    fetched_candidates.extend(response.items);
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
        let (features, feature_degraded) = self
            .load_search_features(request.user_id.as_deref(), &fetched_candidates)
            .await;
        session.degraded |= feature_degraded;
        rerank_results(
            &mut fetched_candidates,
            &plan.original_query,
            plan.intent,
            &features,
        );
        merge_candidates(
            &mut session.pending,
            &mut session.seen_result_ids,
            fetched_candidates,
        );
        sort_results(&mut session.pending);
        Ok(calls)
    }

    async fn load_search_features(
        &self,
        user_id: Option<&str>,
        candidates: &[pb::SearchResult],
    ) -> (HashMap<String, feature_pb::CandidateFeatures>, bool) {
        let Some(user_id) = user_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return (HashMap::new(), false);
        };
        let Some(feature_client) = self.feature_client.as_ref() else {
            return (HashMap::new(), true);
        };
        let content_ids = candidates
            .iter()
            .filter(|candidate| {
                matches!(
                    pb::SearchResultType::try_from(candidate.result_type),
                    Ok(pb::SearchResultType::Post | pb::SearchResultType::Journey)
                )
            })
            .map(|candidate| candidate.id.clone())
            .filter(|id| !id.trim().is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(MAX_FEATURE_RERANK_CANDIDATES)
            .collect::<Vec<_>>();
        if content_ids.is_empty() {
            return (HashMap::new(), false);
        }
        let mut client = feature_client.clone();
        let request = match bookway_runtime::grpc_service_request(feature_pb::FeaturesRequest {
            user_id: user_id.to_string(),
            content_ids,
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, "search feature request authentication degraded");
                return (HashMap::new(), true);
            }
        };
        let response =
            match tokio::time::timeout(FEATURE_RERANK_TIMEOUT, client.features(request)).await {
                Ok(Ok(response)) => response.into_inner(),
                Ok(Err(error)) => {
                    tracing::warn!(%error, user_id, "search feature lookup degraded");
                    return (HashMap::new(), true);
                }
                Err(_) => {
                    tracing::warn!(user_id, "search feature lookup timed out");
                    return (HashMap::new(), true);
                }
            };
        (
            response
                .candidates
                .into_iter()
                .map(|candidate| (candidate.content_id.clone(), candidate))
                .collect(),
            false,
        )
    }

    async fn search_resources(
        &self,
        request: pb::SearchRequest,
    ) -> Result<pb::SearchResponse, SearchMainError> {
        let Some(resource_client) = self.resource_client.as_ref() else {
            return Ok(pb::SearchResponse {
                query: request.q,
                items: Vec::new(),
                next_cursor: None,
                total_estimate: 0,
                took_ms: 0,
                degraded: true,
                request_id: String::new(),
            });
        };
        let mut client = resource_client.clone();
        let query = request.q.clone();
        let response = tokio::time::timeout(
            KNOWLEDGE_CATALOG_TIMEOUT,
            client.search(
                bookway_runtime::grpc_service_request(catalog_pb::SearchRequest {
                    query: query.clone(),
                    kind: None,
                    topic: String::new(),
                    cursor: request.cursor.unwrap_or_default(),
                    limit: request.limit,
                })
                .map_err(|error| SearchMainError::ResourceUpstream {
                    code: tonic::Code::Internal,
                    message: error.to_string(),
                })?,
            ),
        )
        .await
        .map_err(|_| SearchMainError::ResourceUpstream {
            code: tonic::Code::DeadlineExceeded,
            message: "knowledge-catalog request timed out".to_string(),
        })?
        .map_err(|error| SearchMainError::ResourceUpstream {
            code: error.code(),
            message: error.message().to_string(),
        })?
        .into_inner();
        Ok(resource_search_response(query, response))
    }

    /// Embeds the original query through the catalog provider and asks
    /// BBS Search for the nearest indexed documents. Both RPCs are optional:
    /// any failure yields an empty lane rather than a degraded search.
    async fn search_semantic_recall(
        &self,
        request: &pb::SearchRequest,
        plan: &SearchPlan,
    ) -> Result<pb::SearchResponse, SearchMainError> {
        let Some(catalog) = self.resource_client.as_ref() else {
            return Ok(pb::SearchResponse::default());
        };
        let mut client = catalog.clone();
        let embed_request =
            bookway_runtime::grpc_service_request(catalog_pb::EmbedTextsRequest {
                texts: vec![plan.original_query.clone()],
            })
            .map_err(|error| SearchMainError::ResourceUpstream {
                code: tonic::Code::Internal,
                message: error.to_string(),
            })?;
        let embeddings =
            match tokio::time::timeout(SEMANTIC_EMBED_TIMEOUT, client.embed_texts(embed_request))
                .await
            {
                Ok(Ok(response)) => response.into_inner(),
                Ok(Err(error)) => {
                    tracing::debug!(code = %error.code(), "query embedding degraded");
                    return Ok(pb::SearchResponse::default());
                }
                Err(_) => {
                    tracing::debug!("query embedding timed out");
                    return Ok(pb::SearchResponse::default());
                }
            };
        let Some(query_vector) = embeddings
            .embeddings
            .into_iter()
            .next()
            .map(|embedding| embedding.values)
            .filter(|values| !values.is_empty())
        else {
            return Ok(pb::SearchResponse::default());
        };
        let mut bbs = self.search_client.clone();
        let search_request = bookway_runtime::grpc_service_request(pb::SearchSemanticRequest {
            q: plan.original_query.clone(),
            query_vector,
            limit: Some(RECALL_PAGE_SIZE as u32),
            user_id: request.user_id.clone(),
            excluded_author_ids: request.excluded_author_ids.clone(),
            // The semantic lane follows the same typed routing as the lexical
            // BBS lane, so entity tabs and entity-intent queries stay typed.
            search_type: Some(plan.bbs_search_type as i32),
        })
        .map_err(|error| SearchMainError::Upstream {
            code: tonic::Code::Internal,
            message: error.to_string(),
        })?;
        let response =
            match tokio::time::timeout(SEMANTIC_SEARCH_TIMEOUT, bbs.search_semantic(search_request))
                .await
            {
                Ok(Ok(response)) => response.into_inner(),
                Ok(Err(error)) => {
                    tracing::debug!(code = %error.code(), "semantic recall degraded");
                    return Ok(pb::SearchResponse::default());
                }
                Err(_) => {
                    tracing::debug!("semantic recall timed out");
                    return Ok(pb::SearchResponse::default());
                }
            };
        Ok(response)
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

    /// Attaches the live participation facts owned by BBS to route results.
    /// The index never stores a companion count, so the field is absent until
    /// this read fills it. The read is counts-only (anonymous when the searcher
    /// is not signed in) and fails open: a degraded BBS leaves the count
    /// absent — which the client renders as unknown, not as zero companions —
    /// instead of blocking the response.
    async fn hydrate_route_join_counts(
        &self,
        page: &mut [pb::SearchResult],
        user_id: Option<&str>,
    ) {
        let Some(bbs_client) = self.bbs_client.as_ref() else {
            return;
        };
        let route_ids = page
            .iter()
            .filter(|item| item.result_type == pb::SearchResultType::Journey as i32)
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        if route_ids.is_empty() {
            return;
        }
        let request = match bookway_runtime::grpc_service_request(bbs_participation_pb::RouteContextRequest {
            user_id: user_id.unwrap_or_default().to_string(),
            route_ids,
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::debug!(%error, "route join-count hydration skipped");
                return;
            }
        };
        let mut client = bbs_client.clone();
        match tokio::time::timeout(BBS_ROUTE_CONTEXT_TIMEOUT, client.route_context(request)).await {
            Ok(Ok(response)) => {
                let context = response.into_inner();
                for item in page.iter_mut() {
                    if item.result_type != pb::SearchResultType::Journey as i32 {
                        continue;
                    }
                    let Some(post) = item.post.as_mut() else {
                        continue;
                    };
                    post.join_count = context
                        .participant_counts
                        .get(&item.id)
                        .copied()
                        .map(|count| u32::try_from(count).unwrap_or(u32::MAX));
                }
            }
            Ok(Err(error)) => {
                tracing::debug!(code = %error.code(), "route join-count hydration degraded");
            }
            Err(_) => {
                tracing::debug!("route join-count hydration timed out");
            }
        }
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
        let request = bookway_runtime::grpc_service_request(request).map_err(|error| {
            SearchMainError::Upstream {
                code: tonic::Code::Internal,
                message: error.to_string(),
            }
        })?;
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
        let request = bookway_runtime::grpc_service_request(request).map_err(|error| {
            SearchMainError::Upstream {
                code: tonic::Code::Internal,
                message: error.to_string(),
            }
        })?;
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

/// The public content id a result must be revalidated against, or `None` when
/// the result does not stand for a content item at all (users, topics,
/// resources). Node and equipment results are keyed by the action-node or gear
/// label, not by a content id — but they carry the enclosing route's card, so
/// they must be revalidated against THAT route. Skipping them let a deleted or
/// restricted route keep rendering, complete with title, cover and author, in
/// the Nodes and Equipment tabs.
fn revalidation_content_id(candidate: &pb::SearchResult) -> Option<&str> {
    match pb::SearchResultType::try_from(candidate.result_type) {
        Ok(pb::SearchResultType::Post | pb::SearchResultType::Journey) => Some(&candidate.id),
        Ok(pb::SearchResultType::ActionNode | pb::SearchResultType::SceneEquipment) => candidate
            .post
            .as_ref()
            .map(|post| post.id.as_str()),
        _ => None,
    }
}

fn pending_content_ids(
    candidates: &[pb::SearchResult],
) -> Result<BTreeSet<String>, SearchMainError> {
    let mut ids = BTreeSet::new();
    for candidate in candidates {
        let Some(content_id) = revalidation_content_id(candidate) else {
            continue;
        };
        let id = content_id.trim();
        if id.is_empty() || id != content_id {
            return Err(SearchMainError::InvalidContentSummary);
        }
        ids.insert(id.to_string());
    }
    Ok(ids)
}

fn resource_search_response(
    query: String,
    value: catalog_pb::SearchResponse,
) -> pb::SearchResponse {
    let total_estimate = u64::try_from(value.items.len()).unwrap_or(u64::MAX);
    let items = value
        .items
        .into_iter()
        .enumerate()
        .map(|(position, resource)| resource_to_result(resource, position))
        .collect::<Vec<_>>();
    pb::SearchResponse {
        query,
        items,
        next_cursor: value.next_cursor,
        total_estimate,
        took_ms: 0,
        degraded: false,
        request_id: String::new(),
    }
}

fn resource_to_result(resource: catalog_pb::Resource, position: usize) -> pb::SearchResult {
    let kind = match catalog_pb::ResourceKind::try_from(resource.kind).ok() {
        Some(catalog_pb::ResourceKind::Book) => "book",
        Some(catalog_pb::ResourceKind::Course) => "course",
        Some(catalog_pb::ResourceKind::Tool) => "tool",
        Some(catalog_pb::ResourceKind::Article) => "article",
        Some(catalog_pb::ResourceKind::Podcast) => "podcast",
        _ => "unspecified",
    };
    let score = 100.0 - position as f64;
    pb::SearchResult {
        id: resource.id.clone(),
        result_type: pb::SearchResultType::Resource as i32,
        title: resource.title.clone(),
        snippet: resource.summary.clone(),
        cover_url: None,
        author_id: Some(resource.provider.clone()),
        author_name: Some(resource.provider.clone()),
        domain: resource_domain(&resource.topics),
        score,
        highlights: vec![],
        post: None,
        resource: Some(pb::ResourceSummary {
            id: resource.id,
            kind: kind.to_string(),
            provider: resource.provider,
            url: resource.url,
            license: resource.license,
            version: resource.version,
            citation: resource.citation,
            topics: resource.topics,
            published_at: resource.published_at,
            updated_at: resource.updated_at,
        }),
        ad: None,
    }
}

fn resource_domain(topics: &[String]) -> Option<i32> {
    if topics.iter().any(|topic| topic.contains("学习")) {
        Some(pb::GrowthDomain::Learning as i32)
    } else if topics.iter().any(|topic| topic.contains("运动")) {
        Some(pb::GrowthDomain::Movement as i32)
    } else if topics.iter().any(|topic| topic.contains("健康")) {
        Some(pb::GrowthDomain::Wellness as i32)
    } else if topics.iter().any(|topic| topic.contains("旅行")) {
        Some(pb::GrowthDomain::Travel as i32)
    } else {
        None
    }
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
            || post.is_milestone != (content_type == bbs_link_pb::ContentType::Milestone)
            || post.is_question != (content_type == bbs_link_pb::ContentType::Question)
            || authoritative.insert(summary.id.clone(), summary).is_some()
        {
            return Err(SearchMainError::InvalidContentSummary);
        }
    }

    Ok(candidates
        .into_iter()
        .filter_map(|candidate| {
            let Some(content_id) = revalidation_content_id(&candidate).map(str::to_string) else {
                // Users, topics and catalog resources do not stand for a public
                // content item and have nothing to revalidate against.
                return Some(candidate);
            };
            let summary = authoritative.get(&content_id)?;
            match pb::SearchResultType::try_from(candidate.result_type) {
                Ok(pb::SearchResultType::ActionNode | pb::SearchResultType::SceneEquipment) => {
                    // The node/gear identity is the result's own; only the route
                    // card it carries is refreshed from the authoritative read,
                    // and the whole result is dropped when the route is gone.
                    Some(refresh_carried_route_card(candidate, summary))
                }
                _ => Some(search_result_from_summary(candidate, summary)),
            }
        })
        .collect())
}

/// Replaces the enclosing route card on a node or equipment result with the
/// authoritative summary, leaving the node's own identity, title and score
/// untouched. Only routes carry nodes, so a summary whose content type is not
/// a route means the index is stale about this result's shape.
fn refresh_carried_route_card(
    mut candidate: pb::SearchResult,
    summary: &bbs_link_pb::PublicContentSummary,
) -> pb::SearchResult {
    let post = summary
        .post
        .as_ref()
        .expect("authoritative summaries are validated before rebuilding results");
    candidate.author_id = Some(summary.author_id.clone());
    candidate.author_name = Some(post.author_name.clone());
    candidate.cover_url = non_empty(&post.cover_url);
    candidate.domain = Some(search_growth_domain(post.domain));
    candidate.post = Some(search_post_summary(
        post.clone(),
        summary.route_actions.clone(),
    ));
    candidate
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
        post: Some(search_post_summary(
            post.clone(),
            summary.route_actions.clone(),
        )),
        resource: None,
        ad: candidate.ad,
    }
}

fn search_post_summary(
    value: bbs_link_pb::PostSummary,
    route_actions: Vec<bbs_link_pb::RouteTemplateAction>,
) -> pb::PostSummary {
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
        is_milestone: value.is_milestone,
        is_question: value.is_question,
        route_actions,
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
    search_type: pb::SearchType,
    dictionary: &QueryRewriteDictionary,
    enable_resource_search: bool,
) -> Result<SearchPlan, SearchMainError> {
    let original_query = normalize_query(query)?;
    let mut recalls = Vec::new();
    let intent = search_intent(&original_query);
    // Entity-intent queries asked on a general surface search the typed
    // node/equipment indices; an explicit tab (Journeys, Users, …) wins.
    let bbs_search_type = match (search_type, intent) {
        (
            pb::SearchType::All | pb::SearchType::Posts,
            SearchIntent::ActionNode | SearchIntent::Equipment,
        ) => match intent {
            SearchIntent::ActionNode => pb::SearchType::Nodes,
            _ => pb::SearchType::Equipment,
        },
        _ => search_type,
    };
    let aliases = matches!(intent, SearchIntent::Generic | SearchIntent::Journey)
        .then(|| synonym_aliases(&original_query, dictionary))
        .unwrap_or_default();
    if !matches!(search_type, pb::SearchType::Resources) {
        recalls.push(RecallPlan {
            source: RecallSource::Bbs,
            query: original_query.clone(),
        });
        if !aliases.is_empty() {
            let mut expansion_terms = vec![original_query.clone()];
            expansion_terms.extend(aliases.iter().cloned());
            recalls.push(RecallPlan {
                source: RecallSource::Bbs,
                query: expansion_terms.join(" "),
            });
        }
    }
    // Semantic recall covers content surfaces and the typed node/equipment
    // tabs — route documents embed node titles and scene gear in their
    // semantic text, so paraphrased entity queries still recall them. Users,
    // topics and resources have exact lanes that vectors cannot serve.
    if matches!(
        search_type,
        pb::SearchType::All
            | pb::SearchType::Posts
            | pb::SearchType::Journeys
            | pb::SearchType::Nodes
            | pb::SearchType::Equipment
    ) && !matches!(intent, SearchIntent::Topic | SearchIntent::User)
    {
        recalls.push(RecallPlan {
            source: RecallSource::Semantic,
            query: original_query.clone(),
        });
    }
    if enable_resource_search
        && matches!(search_type, pb::SearchType::All | pb::SearchType::Resources)
    {
        recalls.push(RecallPlan {
            source: RecallSource::Resource,
            query: original_query.clone(),
        });
        if !aliases.is_empty() {
            let mut expansion_terms = vec![original_query.clone()];
            expansion_terms.extend(aliases);
            recalls.push(RecallPlan {
                source: RecallSource::Resource,
                query: expansion_terms.join(" "),
            });
        }
    }
    Ok(SearchPlan {
        intent,
        original_query,
        recalls,
        bbs_search_type,
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
    } else if ["资源", "资料", "工具", "课程", "清单"]
        .iter()
        .any(|term| query.contains(term))
    {
        SearchIntent::Resource
    } else if query.contains("节点") {
        SearchIntent::ActionNode
    } else if ["装备", "器材"].iter().any(|term| query.contains(term)) {
        SearchIntent::Equipment
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
                source: recall.source,
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

fn rerank_results(
    items: &mut [pb::SearchResult],
    query: &str,
    intent: SearchIntent,
    features: &HashMap<String, feature_pb::CandidateFeatures>,
) {
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
            SearchIntent::Resource => item.result_type == pb::SearchResultType::Resource as i32,
            SearchIntent::ActionNode => item.result_type == pb::SearchResultType::ActionNode as i32,
            SearchIntent::Equipment => {
                item.result_type == pb::SearchResultType::SceneEquipment as i32
            }
        };
        if matches_intent {
            item.score += 2.0;
        }
        if item.result_type == pb::SearchResultType::Resource as i32 {
            item.score += 0.25;
            if matches!(intent, SearchIntent::Resource) {
                item.score += 1.5;
            }
        }
        if expected_domain.is_some_and(|domain| item.domain == Some(domain)) {
            item.score += 0.5;
        }
        if let Some(candidate) = features.get(&item.id) {
            let p_ctr = finite_probability(candidate.click_through_rate);
            let p_cvr = finite_probability(candidate.purchase_conversion_rate);
            let p_wegu = finite_probability(candidate.action_completion_rate);
            let route_completion = finite_probability(candidate.route_completion_rate);
            // Search remains lexical-first, but verified action completion is
            // the largest behavioral contribution. This favors routes users
            // can finish over results that only attract clicks.
            item.score +=
                3.0 * (0.10 * p_ctr + 0.20 * p_cvr + 0.45 * p_wegu + 0.25 * route_completion);
        }
    }
    sort_results(items);
}

fn finite_probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
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
        Ok(pb::SearchResultType::Topic) => "topic",
        Ok(pb::SearchResultType::Resource) => "resource",
        Ok(pb::SearchResultType::Ad) => "ad",
        Ok(pb::SearchResultType::ActionNode) => "node",
        Ok(pb::SearchResultType::SceneEquipment) | Err(_) => "resource",
    }
}

fn make_cursor(fingerprint: u64, session_id: &str) -> String {
    format!("sm1-{fingerprint:016x}-{session_id}")
}

fn parse_cursor(cursor: Option<&str>, fingerprint: u64) -> Result<Option<String>, SearchMainError> {
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
        return Ok(Some(session_id.to_string()));
    }
    Err(invalid_cursor("搜索游标已过期，请重新搜索"))
}

fn invalid_cursor(message: &str) -> SearchMainError {
    SearchMainError::InvalidCursor(message.to_string())
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
        pb::SearchType::Resources => "resources",
        pb::SearchType::Nodes => "nodes",
        pb::SearchType::Equipment => "equipment",
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

    use bookway_bbs_api::pb::{
        self as bbs_participation_pb,
        bbs_client::BbsClient,
        bbs_server::{Bbs, BbsServer},
    };
    use bookway_bbs_link_api::pb::{
        self as bbs_link_pb,
        bbs_link_client::BbsLinkClient,
        bbs_link_server::{BbsLink, BbsLinkServer},
    };
    use bookway_bbs_search_api::pb::{
        bbs_search_client::BbsSearchClient,
        bbs_search_server::{BbsSearch, BbsSearchServer},
    };
    use bookway_knowledge_catalog_api::pb::{
        self as catalog_pb,
        knowledge_catalog_client::KnowledgeCatalogClient,
        knowledge_catalog_server::{KnowledgeCatalog, KnowledgeCatalogServer},
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

    #[derive(Clone, Default)]
    struct RecordingResourceSource {
        requests: Arc<Mutex<Vec<catalog_pb::SearchRequest>>>,
        responses: Arc<Mutex<HashMap<ResponseKey, catalog_pb::SearchResponse>>>,
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

    impl RecordingResourceSource {
        async fn respond(
            &self,
            query: &str,
            cursor: Option<&str>,
            response: catalog_pb::SearchResponse,
        ) {
            self.responses
                .lock()
                .await
                .insert((query.to_string(), cursor.map(str::to_string)), response);
        }
    }

    #[tonic::async_trait]
    impl BbsSearch for RecordingSearchSource {
        async fn search_semantic(
            &self,
            _request: Request<pb::SearchSemanticRequest>,
        ) -> Result<Response<pb::SearchResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

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
    impl KnowledgeCatalog for RecordingResourceSource {
        async fn embed_texts(
            &self,
            _request: Request<catalog_pb::EmbedTextsRequest>,
        ) -> Result<Response<catalog_pb::EmbedTextsResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn upsert_public_resource(
            &self,
            _request: Request<catalog_pb::UpsertPublicResourceRequest>,
        ) -> Result<Response<catalog_pb::Resource>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn search(
            &self,
            request: Request<catalog_pb::SearchRequest>,
        ) -> Result<Response<catalog_pb::SearchResponse>, Status> {
            let request = request.into_inner();
            self.requests.lock().await.push(request.clone());
            Ok(Response::new(
                self.responses
                    .lock()
                    .await
                    .get(&(
                        request.query.clone(),
                        (!request.cursor.is_empty()).then(|| request.cursor.clone()),
                    ))
                    .cloned()
                    .unwrap_or_default(),
            ))
        }

        async fn get(
            &self,
            _request: Request<catalog_pb::GetRequest>,
        ) -> Result<Response<catalog_pb::Resource>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn list_node_resources(
            &self,
            _request: Request<catalog_pb::ListNodeResourcesRequest>,
        ) -> Result<Response<catalog_pb::ListNodeResourcesResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn attach_node_resource(
            &self,
            _request: Request<catalog_pb::AttachNodeResourceRequest>,
        ) -> Result<Response<catalog_pb::RouteNodeResourceAttachment>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn detach_node_resource(
            &self,
            _request: Request<catalog_pb::DetachNodeResourceRequest>,
        ) -> Result<Response<catalog_pb::DetachNodeResourceResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn retrieve_rag_context(
            &self,
            _request: Request<catalog_pb::RetrieveRagContextRequest>,
        ) -> Result<Response<catalog_pb::RetrieveRagContextResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn upsert_rag_embedding(
            &self,
            _request: Request<catalog_pb::UpsertRagEmbeddingRequest>,
        ) -> Result<Response<catalog_pb::UpsertRagEmbeddingResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn search_rag_embeddings(
            &self,
            _request: Request<catalog_pb::SearchRagEmbeddingsRequest>,
        ) -> Result<Response<catalog_pb::SearchRagEmbeddingsResponse>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
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

        async fn accept_answer(
            &self,
            _request: Request<bbs_link_pb::AcceptAnswerRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }

        async fn fork_route(
            &self,
            _request: Request<bbs_link_pb::ForkRouteRequest>,
        ) -> Result<Response<bbs_link_pb::Content>, Status> {
            Err(Status::unimplemented("not used by Search Main"))
        }
    }

    async fn service(source: Arc<RecordingSearchSource>) -> Domain {
        service_with_exposure_store(
            source,
            Arc::new(MemorySearchExposureStore::default()),
        )
        .await
    }

    async fn service_with_exposure_store(
        source: Arc<RecordingSearchSource>,
        exposures: SharedSearchExposureStore,
    ) -> Domain {
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
            if let Ok(channel) = bookway_runtime::grpc_channel(&endpoint).await {
                let search_client = BbsSearchClient::new(channel);
                return Domain::with_test_dependencies(
                    search_client,
                    Arc::new(MemorySearchSessionStore::default()),
                    exposures.clone(),
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
            if let Ok(channel) = bookway_runtime::grpc_channel(&endpoint).await {
                return BbsLinkClient::new(channel);
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("connect to bbs-link test server");
    }

    async fn resource_client(
        source: RecordingResourceSource,
    ) -> KnowledgeCatalogClient<tonic::transport::Channel> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("allocate test server port");
        let address = listener.local_addr().expect("read test server address");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(KnowledgeCatalogServer::new(source))
                .serve(address)
                .await
                .expect("run knowledge-catalog test server");
        });

        let endpoint = format!("http://{address}");
        for _ in 0..20 {
            if let Ok(channel) = bookway_runtime::grpc_channel(&endpoint).await {
                return KnowledgeCatalogClient::new(channel);
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("connect to knowledge-catalog test server");
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
            resource: None,
            ad: None,
        }
    }

    #[test]
    fn action_completion_outweighs_click_signal_in_search_rerank() {
        let mut items = vec![
            item("click-only", "行动路线", 1.0),
            item("action-proven", "行动路线", 1.0),
        ];
        let features = HashMap::from([
            (
                "click-only".to_string(),
                feature_pb::CandidateFeatures {
                    click_through_rate: 1.0,
                    ..Default::default()
                },
            ),
            (
                "action-proven".to_string(),
                feature_pb::CandidateFeatures {
                    action_completion_rate: 1.0,
                    ..Default::default()
                },
            ),
        ]);

        rerank_results(&mut items, "行动", SearchIntent::Generic, &features);

        assert_eq!(items[0].id, "action-proven");
    }

    #[test]
    fn route_completion_outweighs_click_signal_in_search_rerank() {
        let mut items = vec![
            item("click-only", "行动路线", 1.0),
            item("route-proven", "行动路线", 1.0),
        ];
        let features = HashMap::from([
            (
                "click-only".to_string(),
                feature_pb::CandidateFeatures {
                    click_through_rate: 1.0,
                    ..Default::default()
                },
            ),
            (
                "route-proven".to_string(),
                feature_pb::CandidateFeatures {
                    route_completion_rate: 1.0,
                    ..Default::default()
                },
            ),
        ]);

        rerank_results(&mut items, "行动", SearchIntent::Generic, &features);

        assert_eq!(items[0].id, "route-proven");
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

    fn catalog_resource(id: &str, title: &str, summary: &str) -> catalog_pb::Resource {
        catalog_pb::Resource {
            id: id.to_string(),
            title: title.to_string(),
            kind: catalog_pb::ResourceKind::Course as i32,
            provider: "开放课程机构".to_string(),
            summary: summary.to_string(),
            url: format!("https://resources.example/{id}"),
            license: "CC BY-NC-SA 4.0".to_string(),
            version: "2026.1".to_string(),
            citation: format!("开放课程机构. {title}. 2026."),
            topics: vec!["学习".to_string(), "课程".to_string()],
            status: catalog_pb::ResourceStatus::Published as i32,
            published_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
        }
    }

    /// Node and equipment results are keyed by the action-node id or the gear
    /// label, so they used to slip past revalidation entirely — a deleted or
    /// restricted route kept rendering its title, cover and author in the Nodes
    /// and Equipment tabs. They must be revalidated against the route card they
    /// carry, and dropped with it.
    #[tokio::test]
    async fn node_and_equipment_results_are_revalidated_against_their_route() {
        let carried = |result_type: pb::SearchResultType, id: &str, route_id: &str| {
            pb::SearchResult {
                id: id.to_string(),
                result_type: result_type as i32,
                title: "索引里的节点标题".to_string(),
                snippet: "索引快照".to_string(),
                cover_url: Some("https://cdn.example/stale.jpg".to_string()),
                author_id: Some("stale-author".to_string()),
                author_name: Some("索引里的作者".to_string()),
                domain: Some(pb::GrowthDomain::Learning as i32),
                score: 3.5,
                highlights: vec!["节点命中".to_string()],
                post: Some(pb::PostSummary {
                    id: route_id.to_string(),
                    title: "索引里的路线标题".to_string(),
                    ..Default::default()
                }),
                resource: None,
                ad: None,
            }
        };

        let reconciled = reconcile_pending_results(
            vec![
                carried(pb::SearchResultType::ActionNode, "node-1", "route-live"),
                carried(
                    pb::SearchResultType::SceneEquipment,
                    "跑鞋",
                    "route-withdrawn",
                ),
            ],
            bbs_link_pb::PublicContentSummaries {
                items: vec![public_summary(
                    "route-live",
                    "route-author",
                    bbs_link_pb::ContentType::Route,
                    "当前路线标题",
                    "当前路线摘要",
                    bbs_link_pb::GrowthDomain::Travel,
                )],
            },
        )
        .expect("reconciliation should accept a valid authoritative read");

        assert_eq!(
            reconciled.len(),
            1,
            "a node whose route is no longer public must be dropped"
        );
        let node = &reconciled[0];
        // The node keeps its own identity, title and score...
        assert_eq!(node.id, "node-1");
        assert_eq!(node.result_type, pb::SearchResultType::ActionNode as i32);
        assert_eq!(node.title, "索引里的节点标题");
        assert!((node.score - 3.5).abs() < f64::EPSILON);
        // ...while the route card it carries comes from the authoritative read.
        assert_eq!(node.author_id.as_deref(), Some("route-author"));
        assert_eq!(node.author_name.as_deref(), Some("当前作者-route-live"));
        assert_eq!(node.domain, Some(pb::GrowthDomain::Travel as i32));
        assert_eq!(
            node.post.as_ref().map(|post| post.title.as_str()),
            Some("当前路线标题")
        );
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
                join_count: None,
                like_count: 34,
                freshness: 0.8,
                tags: vec!["当前标签".to_string()],
                is_route: content_type == bbs_link_pb::ContentType::Route,
                is_milestone: content_type == bbs_link_pb::ContentType::Milestone,
                is_question: content_type == bbs_link_pb::ContentType::Question,
                fork_count: 0,
            }),
            author_id: author_id.to_string(),
            content_type: content_type as i32,
            topics: vec!["当前话题".to_string()],
            quality_score: 0.9,
            route_actions: Vec::new(),
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

    #[tokio::test]
    async fn rejects_oversized_route_context_fields() {
        let source = Arc::new(RecordingSearchSource::default());
        let service = service(source).await;
        let mut request = request("路线");
        request.route_id = Some("r".repeat(MAX_ROUTE_CONTEXT_FIELD_LENGTH + 1));
        request.action_node_id = Some("node-1".to_string());
        let error = service
            .search(request)
            .await
            .expect_err("oversized route context must be rejected");
        assert!(matches!(error, SearchMainError::InvalidCursor(_)));
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
            resource: None,
            ad: None,
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
            resource: None,
            ad: None,
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
        let plan = make_search_plan("跑步 计划", pb::SearchType::All, &dictionary, false)
            .expect("query plan should build");
        assert_eq!(plan.query_rewrite_version, "lifestyle-v3");
        // Exact + synonym expansion + one-shot semantic lane.
        assert_eq!(plan.recalls.len(), 3);
        assert_eq!(plan.recalls[1].query, "跑步 计划 慢跑 晨跑");
        assert_eq!(plan.recalls[2].source, crate::datasource::RecallSource::Semantic);
        assert_eq!(new_session(1, &plan).query_rewrite_version, "lifestyle-v3");

        let identity_plan = make_search_plan("#跑步", pb::SearchType::All, &dictionary, false)
            .expect("topic query plan should build");
        assert_eq!(identity_plan.recalls.len(), 1);
    }

    #[test]
    fn entity_tabs_plan_a_semantic_lane_for_typed_recall() {
        let dictionary = QueryRewriteDictionary {
            version: "lifestyle-v3".to_string(),
            rules: Vec::new(),
        };
        for search_type in [pb::SearchType::Nodes, pb::SearchType::Equipment] {
            let plan = make_search_plan("壶铃", search_type, &dictionary, false)
                .expect("entity tab plan should build");
            assert_eq!(plan.recalls.len(), 2);
            assert_eq!(plan.recalls[0].source, crate::datasource::RecallSource::Bbs);
            assert_eq!(plan.recalls[1].source, crate::datasource::RecallSource::Semantic);
            assert_eq!(plan.bbs_search_type, search_type);
        }

        // Identity surfaces keep their exact lanes.
        let topic_plan = make_search_plan("#壶铃", pb::SearchType::Nodes, &dictionary, false)
            .expect("topic query plan should build");
        assert_eq!(topic_plan.recalls.len(), 1);
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

    struct FailingQueryRewriteDao;

    #[async_trait::async_trait]
    impl crate::datasource::QueryRewriteDao for FailingQueryRewriteDao {
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
        let cache = QueryRewriteCache::new(Arc::new(FailingQueryRewriteDao));

        let resolution = cache.active().await;

        assert!(resolution.degraded);
        assert_eq!(resolution.dictionary.version, "builtin-v1");
        assert_eq!(
            make_search_plan("跑步", pb::SearchType::All, &resolution.dictionary, false)
                .expect("fallback dictionary remains usable")
                .recalls
                .len(),
            3
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
    async fn route_equipment_queries_expand_through_the_versioned_dictionary() {
        let source = Arc::new(RecordingSearchSource::default());
        let service = service(source.clone()).await;

        service
            .search(request("登山鞋"))
            .await
            .expect("equipment search works");

        let queries = source
            .requests
            .lock()
            .await
            .iter()
            .map(|request| request.q.clone())
            .collect::<Vec<_>>();
        assert_eq!(queries, vec!["登山鞋", "登山鞋 徒步鞋 越野鞋 防滑鞋"]);
    }

    #[tokio::test]
    async fn resources_search_uses_public_catalog_without_bbs_recall() {
        let search_source = Arc::new(RecordingSearchSource::default());
        let resource_source = RecordingResourceSource::default();
        resource_source
            .respond(
                "课程资源",
                None,
                catalog_pb::SearchResponse {
                    items: vec![catalog_resource(
                        "resource-course-1",
                        "行动复盘公开课程",
                        "围绕行动记录和周复盘设计的公开课程。",
                    )],
                    next_cursor: None,
                },
            )
            .await;
        let mut service = service(search_source.clone()).await;
        service.resource_client = Some(resource_client(resource_source.clone()).await);

        let mut search_request = request("课程资源");
        search_request.search_type = pb::SearchType::Resources as i32;
        let page = service
            .search(search_request)
            .await
            .expect("resource search succeeds");

        assert!(
            search_source.requests.lock().await.is_empty(),
            "resource-only search must not query BBS"
        );
        assert_eq!(resource_source.requests.lock().await[0].query, "课程资源");
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].result_type,
            pb::SearchResultType::Resource as i32
        );
        assert_eq!(page.items[0].title, "行动复盘公开课程");
        assert_eq!(
            page.items[0]
                .resource
                .as_ref()
                .map(|resource| resource.url.as_str()),
            Some("https://resources.example/resource-course-1")
        );
        assert_eq!(
            page.items[0].domain,
            Some(pb::GrowthDomain::Learning as i32)
        );
    }

    #[tokio::test]
    async fn contextual_search_does_not_recall_unscoped_resources() {
        let search_source = Arc::new(RecordingSearchSource::default());
        let resource_source = RecordingResourceSource::default();
        let resource_requests = resource_source.requests.clone();
        let mut service = service(search_source).await;
        service.resource_client = Some(resource_client(resource_source).await);

        let mut search_request = request("课程资源");
        search_request.route_id = Some("route-1".to_string());
        search_request.action_node_id = Some("action-1".to_string());
        search_request.scene_equipment = Some("瑜伽垫".to_string());
        service
            .search(search_request)
            .await
            .expect("contextual search succeeds");

        assert!(
            resource_requests.lock().await.is_empty(),
            "route-node searches must not mix in unscoped catalog resources"
        );
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

    #[test]
    fn entity_intents_route_general_surfaces_to_typed_bbs_indices() {
        let dictionary = QueryRewriteDictionary {
            version: "test-1".to_string(),
            rules: Vec::new(),
        };
        let plan = make_search_plan(
            "徒步装备",
            pb::SearchType::All,
            &dictionary,
            false,
        )
        .expect("plan builds");
        assert_eq!(plan.intent, SearchIntent::Equipment);
        assert_eq!(plan.bbs_search_type, pb::SearchType::Equipment);

        let node_plan = make_search_plan(
            "晨跑打卡节点",
            pb::SearchType::Posts,
            &dictionary,
            false,
        )
        .expect("node plan builds");
        assert_eq!(node_plan.intent, SearchIntent::ActionNode);
        assert_eq!(node_plan.bbs_search_type, pb::SearchType::Nodes);

        // An explicit tab keeps its type even when entity words appear.
        let journey_plan = make_search_plan(
            "徒步路线装备",
            pb::SearchType::Journeys,
            &dictionary,
            false,
        )
        .expect("journey plan builds");
        assert_eq!(journey_plan.intent, SearchIntent::Journey);
        assert_eq!(journey_plan.bbs_search_type, pb::SearchType::Journeys);
    }

    #[tokio::test]
    async fn equipment_intent_queries_recall_the_typed_equipment_index() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond("徒步装备", None, response(Vec::new(), None))
            .await;
        let service = service(source.clone()).await;

        service
            .search(request("徒步装备"))
            .await
            .expect("entity search succeeds");

        let requests = source.requests.lock().await;
        assert!(
            !requests.is_empty(),
            "the BBS recall must run for an equipment query"
        );
        assert!(
            requests
                .iter()
                .all(|sent| sent.search_type == pb::SearchType::Equipment as i32),
            "every BBS recall round must search the typed equipment index"
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
    async fn anonymous_search_does_not_write_a_shared_exposure_identity() {
        let source = Arc::new(RecordingSearchSource::default());
        source
            .respond(
                "阅读",
                None,
                response(vec![item("post-1", "阅读笔记", 1.0)], None),
            )
            .await;
        let exposures = Arc::new(MemorySearchExposureStore::default());
        let service = service_with_exposure_store(source, exposures.clone()).await;

        let search_response = service
            .search(request("阅读"))
            .await
            .expect("anonymous search works");
        assert!(!search_response.request_id.is_empty());
        assert_eq!(
            exposures.len().await,
            0,
            "anonymous traffic must not share one synthetic attribution row"
        );
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
    async fn legacy_search_cursor_is_rejected() {
        let source = Arc::new(RecordingSearchSource::default());
        let service = service(source).await;
        let mut request = request("跑步");
        request.cursor =
            Some("v3-0000000000000000-018f5e6e-f3e6-7b5f-8e16-5c93f8f5ba88".to_string());
        assert!(matches!(
            service.search(request).await,
            Err(SearchMainError::InvalidCursor(_))
        ));
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

    #[derive(Clone, Default)]
    struct StubParticipationSource {
        counts: Arc<Mutex<HashMap<String, u64>>>,
        seen_user_ids: Arc<Mutex<Vec<String>>>,
    }

    impl StubParticipationSource {
        async fn last_seen_user_id(&self) -> Option<String> {
            self.seen_user_ids.lock().await.last().cloned()
        }
    }

    #[async_trait::async_trait]
    impl Bbs for StubParticipationSource {
        async fn context(
            &self,
            _request: Request<bbs_participation_pb::ContextRequest>,
        ) -> Result<Response<bbs_participation_pb::SocialContext>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn visibility_context(
            &self,
            _request: Request<bbs_participation_pb::ContextRequest>,
        ) -> Result<Response<bbs_participation_pb::SocialVisibility>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn set_edge(
            &self,
            _request: Request<bbs_participation_pb::SetEdgeRequest>,
        ) -> Result<Response<bbs_participation_pb::SocialContext>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn list_route_participations(
            &self,
            _request: Request<bbs_participation_pb::ContextRequest>,
        ) -> Result<Response<bbs_participation_pb::RouteParticipationList>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn route_context(
            &self,
            request: Request<bbs_participation_pb::RouteContextRequest>,
        ) -> Result<Response<bbs_participation_pb::RouteParticipationContext>, Status> {
            let request = request.into_inner();
            self.seen_user_ids.lock().await.push(request.user_id);
            let counts = self.counts.lock().await;
            // Mirror the real read: only the requested routes come back.
            let participant_counts = request
                .route_ids
                .iter()
                .filter_map(|route_id| counts.get(route_id).map(|count| (route_id.clone(), *count)))
                .collect();
            Ok(Response::new(bbs_participation_pb::RouteParticipationContext {
                participant_counts,
                joined_route_ids: Vec::new(),
            }))
        }

        async fn set_route_participation(
            &self,
            _request: Request<bbs_participation_pb::RouteParticipationRequest>,
        ) -> Result<Response<bbs_participation_pb::RouteParticipationState>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn list_followers(
            &self,
            _request: Request<bbs_participation_pb::ListFollowersRequest>,
        ) -> Result<Response<bbs_participation_pb::FollowerPage>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn get_social_stats(
            &self,
            _request: Request<bbs_participation_pb::SocialStatsRequest>,
        ) -> Result<Response<bbs_participation_pb::SocialStats>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }

        async fn list_route_peers(
            &self,
            _request: Request<bbs_participation_pb::ListRoutePeersRequest>,
        ) -> Result<Response<bbs_participation_pb::RoutePeerPage>, Status> {
            Err(Status::unimplemented("not used by join-count hydration"))
        }
    }

    async fn participation_client(
        counts: HashMap<String, u64>,
    ) -> (BbsClient<tonic::transport::Channel>, StubParticipationSource) {
        let source = StubParticipationSource {
            counts: Arc::new(Mutex::new(counts)),
            seen_user_ids: Arc::new(Mutex::new(Vec::new())),
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("allocate test server port");
        let address = listener.local_addr().expect("read test server address");
        drop(listener);
        let server_source = source.clone();
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(BbsServer::new(server_source))
                .serve(address)
                .await
                .expect("run bbs participation test server");
        });

        let endpoint = format!("http://{address}");
        for _ in 0..20 {
            if let Ok(channel) = bookway_runtime::grpc_channel(&endpoint).await {
                return (BbsClient::new(channel), source);
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("connect to bbs participation test server");
    }

    fn route_result(id: &str) -> pb::SearchResult {
        pb::SearchResult {
            id: id.to_string(),
            result_type: pb::SearchResultType::Journey as i32,
            post: Some(pb::PostSummary {
                id: id.to_string(),
                join_count: None,
                is_route: true,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// A channel to a socket that accepts TCP but serves nothing: hydration
    /// tests only need the search client to be constructible, never usable.
    async fn idle_channel() -> tonic::transport::Channel {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("allocate idle test port");
        let address = listener.local_addr().expect("read idle test address");
        tokio::spawn(async move {
            let _listener = listener;
            std::future::pending::<()>().await;
        });
        bookway_runtime::grpc_channel(&format!("http://{address}"))
            .await
            .expect("connect idle test channel")
    }

    #[tokio::test]
    async fn hydration_attaches_live_participation_facts_and_leaves_the_rest_absent() {
        let (client, source) = participation_client(HashMap::from([(
            "route-live".to_string(),
            256,
        )]))
        .await;
        let mut domain = Domain::with_test_dependencies(
            BbsSearchClient::new(idle_channel().await),
            Arc::new(MemorySearchSessionStore::default()),
            Arc::new(MemorySearchExposureStore::default()),
        );
        domain.bbs_client = Some(client);

        let mut page = vec![
            route_result("route-live"),
            route_result("route-absent"),
            pb::SearchResult {
                id: "post-plain".to_string(),
                result_type: pb::SearchResultType::Post as i32,
                post: Some(pb::PostSummary {
                    id: "post-plain".to_string(),
                    join_count: Some(3),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        domain.hydrate_route_join_counts(&mut page, Some("walker")).await;

        assert_eq!(
            page[0].post.as_ref().and_then(|post| post.join_count),
            Some(256)
        );
        assert_eq!(
            page[1].post.as_ref().and_then(|post| post.join_count),
            None,
            "a route BBS did not answer for claims no companion count at all"
        );
        assert_eq!(
            page[2].post.as_ref().and_then(|post| post.join_count),
            Some(3),
            "non-route results are never hydrated"
        );
        assert_eq!(source.last_seen_user_id().await.as_deref(), Some("walker"));
    }

    #[tokio::test]
    async fn hydration_fails_open_without_a_bbs_connection() {
        let mut domain = Domain::with_test_dependencies(
            BbsSearchClient::new(idle_channel().await),
            Arc::new(MemorySearchSessionStore::default()),
            Arc::new(MemorySearchExposureStore::default()),
        );
        domain.bbs_client = None;

        let mut page = vec![route_result("route-live")];
        domain.hydrate_route_join_counts(&mut page, None).await;
        assert_eq!(
            page[0].post.as_ref().and_then(|post| post.join_count),
            None,
            "without BBS the response must not invent a companion count"
        );
    }
}
