use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Instant,
};

use bookway_api::{
    GrowthDomainDto, SearchQueryRequest, SearchResponseDto, SearchResultDto, SearchResultTypeDto,
    SearchTypeDto, SuggestionQueryRequest, SuggestionResponseDto,
};
use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        GrpcSearchDataSource, MemorySearchSessionStore, PostgresSearchSessionStore, RecallState,
        SearchClientError, SearchDataSource, SearchPipelineSession, SearchSessionError,
        SearchSessionStore,
    },
};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;
const RECALL_PAGE_SIZE: usize = 50;
const MAX_RECALL_PAGES_PER_RESPONSE: usize = 8;
const MAX_PUBLIC_CURSOR_BYTES: usize = 128;
const MAX_QUERY_LENGTH: usize = 100;

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
    #[error(transparent)]
    Upstream(#[from] SearchClientError),
}

#[derive(Clone)]
pub(crate) struct SearchMainService {
    search: Arc<dyn SearchDataSource>,
    sessions: Arc<dyn SearchSessionStore>,
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) service: SearchMainService,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let source = GrpcSearchDataSource::connect(config.bbs_search_url.clone()).await?;
        let sessions: Arc<dyn SearchSessionStore> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemorySearchSessionStore::default()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresSearchSessionStore::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        Ok(Self {
            config,
            service: SearchMainService::with_dependencies(Arc::new(source), sessions),
        })
    }
}

impl SearchMainService {
    #[cfg(test)]
    pub(crate) fn new(search: Arc<dyn SearchDataSource>) -> Self {
        Self::with_dependencies(search, Arc::new(MemorySearchSessionStore::default()))
    }

    pub(crate) fn with_dependencies(
        search: Arc<dyn SearchDataSource>,
        sessions: Arc<dyn SearchSessionStore>,
    ) -> Self {
        Self { search, sessions }
    }

    pub(crate) async fn search(
        &self,
        mut request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchMainError> {
        let started = Instant::now();
        let plan = make_search_plan(&request.q)?;
        request.q = plan.original_query.clone();
        request.limit = Some(
            request
                .limit
                .unwrap_or(DEFAULT_PAGE_SIZE)
                .clamp(1, MAX_PAGE_SIZE),
        );
        request.excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let fingerprint = query_fingerprint(
            &plan.original_query,
            request.search_type,
            request.user_id.as_deref(),
            &request.excluded_author_ids,
        );
        let cursor = parse_cursor(
            request.cursor.as_deref(),
            fingerprint,
            &plan.original_query,
            request.search_type,
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

        let mut page = Vec::with_capacity(request.limit.unwrap_or(DEFAULT_PAGE_SIZE));
        let limit = request.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        let mut source_calls = 0;
        while page.len() < limit {
            if !session.pending.is_empty() {
                let take = (limit - page.len()).min(session.pending.len());
                page.extend(session.pending.drain(..take));
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
        tracing::debug!(
            query_hash = format_args!("{:016x}", stable_hash(&plan.original_query)),
            variants = session.recalls.len(),
            source_calls,
            candidates = session.delivered_count + session.pending.len(),
            took_ms = started.elapsed().as_millis() as u64,
            degraded = session.degraded,
            "search pipeline completed"
        );
        Ok(SearchResponseDto {
            query: plan.original_query,
            items: page,
            next_cursor,
            total_estimate,
            took_ms: started.elapsed().as_millis() as u64,
            degraded: session.degraded,
        })
    }

    async fn fetch_recall_round(
        &self,
        request: &SearchQueryRequest,
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
            source_request.limit = Some(RECALL_PAGE_SIZE);
            calls += 1;

            match self.search.search(source_request).await {
                Ok(response) => {
                    let recall = &mut session.recalls[index];
                    recall.source_cursor = response.next_cursor;
                    recall.exhausted = recall.source_cursor.is_none();
                    session.source_total_estimate =
                        session.source_total_estimate.max(response.total_estimate);
                    session.degraded |= response.degraded;
                    let mut candidates = response.items;
                    rerank_results(&mut candidates, &plan.original_query, plan.intent);
                    merge_candidates(
                        &mut session.pending,
                        &mut session.seen_result_ids,
                        candidates,
                    );
                }
                Err(error) if index == 0 => return Err(error.into()),
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

    pub(crate) async fn suggestions(
        &self,
        mut request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, SearchMainError> {
        request.q = normalize_query(&request.q)?;
        request.excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        Ok(self.search.suggestions(request).await?)
    }
}

fn make_search_plan(query: &str) -> Result<SearchPlan, SearchMainError> {
    let original_query = normalize_query(query)?;
    let mut recalls = vec![RecallPlan {
        query: original_query.clone(),
    }];
    let aliases = synonym_aliases(&original_query);
    if !aliases.is_empty() {
        let mut expansion_terms = vec![original_query.clone()];
        expansion_terms.extend(aliases);
        recalls.push(RecallPlan {
            query: expansion_terms.join(" "),
        });
    }
    Ok(SearchPlan {
        intent: search_intent(&original_query),
        original_query,
        recalls,
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

fn synonym_aliases(query: &str) -> Vec<String> {
    const SYNONYMS: [(&str, [&str; 3]); 6] = [
        ("跑步", ["慢跑", "晨跑", "夜跑"]),
        ("阅读", ["读书", "书单", "主题阅读"]),
        ("睡眠", ["早睡", "作息", "睡眠修复"]),
        ("冥想", ["正念", "呼吸", "静坐"]),
        ("旅行", ["徒步", "城市漫游", "出行"]),
        ("徒步", ["登山", "步道", "远足"]),
    ];
    let mut aliases = Vec::new();
    for (term, terms) in SYNONYMS {
        if query.contains(term) {
            for alias in terms {
                if !query.contains(alias) && !aliases.contains(&alias.to_string()) {
                    aliases.push(alias.to_string());
                }
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
    pending: &mut Vec<SearchResultDto>,
    seen_result_ids: &mut HashSet<String>,
    candidates: Vec<SearchResultDto>,
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

fn rerank_results(items: &mut [SearchResultDto], query: &str, intent: SearchIntent) {
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
        if matches!(
            (intent, item.result_type),
            (SearchIntent::Topic, SearchResultTypeDto::Topic)
                | (SearchIntent::User, SearchResultTypeDto::User)
                | (SearchIntent::Journey, SearchResultTypeDto::Journey)
        ) {
            item.score += 2.0;
        }
        if expected_domain.is_some_and(|domain| item.domain == Some(domain)) {
            item.score += 0.5;
        }
    }
    sort_results(items);
}

fn sort_results(items: &mut [SearchResultDto]) {
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

fn domain_for_query(query: &str) -> Option<GrowthDomainDto> {
    if ["跑步", "慢跑", "晨跑", "夜跑", "徒步", "登山", "步道"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(GrowthDomainDto::Movement)
    } else if ["阅读", "读书", "书单", "学习"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(GrowthDomainDto::Learning)
    } else if ["睡眠", "早睡", "冥想", "正念", "作息"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(GrowthDomainDto::Wellness)
    } else if ["旅行", "城市漫游", "出行"]
        .iter()
        .any(|term| query.contains(term))
    {
        Some(GrowthDomainDto::Travel)
    } else {
        None
    }
}

fn result_identity(item: &SearchResultDto) -> String {
    format!("{}:{}", result_type_name(item.result_type), item.id)
}

fn result_type_name(result_type: SearchResultTypeDto) -> &'static str {
    match result_type {
        SearchResultTypeDto::Post => "post",
        SearchResultTypeDto::Journey => "journey",
        SearchResultTypeDto::User => "user",
        SearchResultTypeDto::Topic => "topic",
    }
}

fn make_cursor(fingerprint: u64, session_id: &str) -> String {
    format!("sm1-{fingerprint:016x}-{session_id}")
}

fn parse_cursor(
    cursor: Option<&str>,
    fingerprint: u64,
    query: &str,
    search_type: SearchTypeDto,
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
    search_type: SearchTypeDto,
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
    search_type: SearchTypeDto,
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

fn search_type_name(search_type: SearchTypeDto) -> &'static str {
    match search_type {
        SearchTypeDto::All => "all",
        SearchTypeDto::Posts => "posts",
        SearchTypeDto::Journeys => "journeys",
        SearchTypeDto::Users => "users",
        SearchTypeDto::Topics => "topics",
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use async_trait::async_trait;
    use bookway_api::{
        GrowthDomainDto, SearchResponseDto, SearchResultDto, SearchResultTypeDto,
        SuggestionQueryRequest, SuggestionResponseDto,
    };
    use tokio::sync::Mutex;

    use super::*;
    use crate::datasource::{SearchClientError, SearchDataSource};

    type ResponseKey = (String, Option<String>);
    type RecordedResponse = Result<SearchResponseDto, String>;

    #[derive(Default)]
    struct RecordingSearchSource {
        requests: Mutex<Vec<SearchQueryRequest>>,
        responses: Mutex<HashMap<ResponseKey, RecordedResponse>>,
    }

    impl RecordingSearchSource {
        async fn respond(&self, query: &str, cursor: Option<&str>, response: SearchResponseDto) {
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
    }

    #[async_trait]
    impl SearchDataSource for RecordingSearchSource {
        async fn search(
            &self,
            request: SearchQueryRequest,
        ) -> Result<SearchResponseDto, SearchClientError> {
            self.requests.lock().await.push(request.clone());
            self.responses
                .lock()
                .await
                .get(&(request.q, request.cursor))
                .cloned()
                .unwrap_or_else(|| Ok(response(Vec::new(), None)))
                .map_err(SearchClientError::Transport)
        }

        async fn suggestions(
            &self,
            _request: SuggestionQueryRequest,
        ) -> Result<SuggestionResponseDto, SearchClientError> {
            Ok(SuggestionResponseDto {
                query: String::new(),
                items: Vec::new(),
            })
        }
    }

    fn service(source: Arc<RecordingSearchSource>) -> SearchMainService {
        SearchMainService::new(source)
    }

    fn request(query: &str) -> SearchQueryRequest {
        SearchQueryRequest {
            q: query.to_string(),
            limit: Some(20),
            ..Default::default()
        }
    }

    fn item(id: &str, title: &str, score: f64) -> SearchResultDto {
        SearchResultDto {
            id: id.to_string(),
            result_type: SearchResultTypeDto::Post,
            title: title.to_string(),
            snippet: String::new(),
            cover_url: None,
            author_id: None,
            author_name: None,
            domain: Some(GrowthDomainDto::Movement),
            score,
            highlights: Vec::new(),
            post: None,
        }
    }

    fn response(items: Vec<SearchResultDto>, next_cursor: Option<&str>) -> SearchResponseDto {
        SearchResponseDto {
            query: String::new(),
            total_estimate: items.len(),
            items,
            next_cursor: next_cursor.map(str::to_string),
            took_ms: 0,
            degraded: false,
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
    async fn recalls_canonical_and_known_synonym_queries() {
        let source = Arc::new(RecordingSearchSource::default());
        let service = service(source.clone());

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
        let service = service(source);
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
        let service = service(source);
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
        changed.search_type = SearchTypeDto::Posts;
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
        let fingerprint = query_fingerprint("跑步", SearchTypeDto::All, None, &[]);
        let legacy_cursor = format!("v3-{fingerprint:016x}-{legacy_id}");
        source
            .respond(
                "跑步",
                Some(&legacy_cursor),
                response(vec![item("a", "跑步", 1.0)], Some("v3-next")),
            )
            .await;
        let service = service(source.clone());
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
            .search(request("跑步"))
            .await
            .expect("exact recall remains available");

        assert_eq!(result.items.len(), 1);
        assert!(result.degraded);
    }
}
