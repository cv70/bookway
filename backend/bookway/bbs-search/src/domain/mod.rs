use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use bookway_api::{
    ContentDto, ContentQueryRequest, ContentStatusDto, ContentTypeDto, PostSummaryDto,
    SearchResultDto, SearchResultTypeDto, SearchTypeDto, SuggestionDto,
};
use thiserror::Error;

use super::{
    api::{SearchQueryRequest, SearchResponseDto, SuggestionQueryRequest, SuggestionResponseDto},
    datasource::{
        GrpcContentSearchSource, MemorySearchAnalytics, MemorySearchSessionStore, OpenSearchSource,
        PostgresSearchAnalytics, PostgresSearchSessionStore, SearchSession, SearchSessionStore,
        SearchSource, SearchSourceError, SharedSearchAnalytics, search_type_name, stable_hash,
    },
};
use crate::conf::Config;

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;
const SOURCE_PAGE_SIZE: usize = 100;
const MAX_SOURCE_PAGES_PER_RESPONSE: usize = 20;
const MAX_PUBLIC_CURSOR_BYTES: usize = 128;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) search: SearchService,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let fallback = GrpcContentSearchSource::connect(config.bbs_link_url.clone()).await?;
        let source: Arc<dyn SearchSource> = match config.opensearch_url.clone() {
            Some(url) => Arc::new(OpenSearchSource::new(
                url,
                config.opensearch_index.clone(),
                fallback,
            )),
            None => Arc::new(fallback),
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
        })
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
    source: Arc<dyn SearchSource>,
    analytics: SharedSearchAnalytics,
    sessions: Arc<dyn SearchSessionStore>,
    popular_terms: Arc<Vec<String>>,
}

impl SearchService {
    #[cfg(test)]
    pub(crate) fn new(source: Arc<dyn SearchSource>) -> Self {
        Self::with_dependencies(
            source,
            Arc::new(MemorySearchAnalytics::default()),
            Arc::new(MemorySearchSessionStore::default()),
        )
    }

    pub(crate) fn with_dependencies(
        source: Arc<dyn SearchSource>,
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

    pub(crate) async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchError> {
        let query_text = request.q.trim().to_string();
        if query_text.is_empty() || query_text.chars().count() > 100 {
            return Err(SearchError::Validation(
                "搜索词需要在 1 到 100 个字符之间".to_string(),
            ));
        }
        let excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let excluded_authors = excluded_author_ids.iter().cloned().collect::<HashSet<_>>();
        let fingerprint = query_fingerprint(
            &query_text,
            request.search_type,
            request.user_id.as_deref(),
            &excluded_author_ids,
        );
        let session_id = parse_cursor(
            request.cursor.as_deref(),
            &query_text,
            request.search_type,
            request.user_id.as_deref(),
            &excluded_author_ids,
        )?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
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
            let source_result = self
                .source
                .search_contents(
                    ContentQueryRequest {
                        cursor: session.source_cursor.clone(),
                        limit: Some(SOURCE_PAGE_SIZE),
                        status: Some(ContentStatusDto::Published),
                        strategy: Some("fresh".to_string()),
                        ids: None,
                        author_id: None,
                        content_type: match request.search_type {
                            SearchTypeDto::Journeys => Some(ContentTypeDto::Route),
                            _ => None,
                        },
                        domain: None,
                    },
                    &query_text,
                    &excluded_author_ids,
                )
                .await
                .map_err(map_source_error)?;
            source_pages += 1;
            session.source_cursor = source_result.page.next_cursor;
            session.source_exhausted = session.source_cursor.is_none();
            session.source_total_estimate = session
                .source_total_estimate
                .max(source_result.page.total_estimate);
            session.degraded |= source_result.degraded;
            let mut candidates = search_results(
                &source_result.page.items,
                &query_text,
                request.search_type,
                &excluded_authors,
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
                request.search_type,
                request.user_id.as_deref(),
                &excluded_author_ids,
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
                    &query_text,
                    request.search_type,
                    !has_next_page && page.is_empty(),
                )
                .await;
        }
        Ok(SearchResponseDto {
            query: query_text,
            items: page,
            next_cursor,
            total_estimate,
            took_ms: started.elapsed().as_millis() as u64,
            degraded: session.degraded,
        })
    }

    pub(crate) async fn suggestions(
        &self,
        request: SuggestionQueryRequest,
    ) -> Result<SuggestionResponseDto, SearchError> {
        let query = request.q.trim().to_string();
        if query.is_empty() {
            return Ok(SuggestionResponseDto {
                query,
                items: Vec::new(),
            });
        }
        let excluded_author_ids = normalize_excluded_author_ids(&request.excluded_author_ids);
        let excluded_authors = excluded_author_ids.iter().cloned().collect::<HashSet<_>>();
        let source_query = ContentQueryRequest {
            limit: Some(30),
            status: Some(ContentStatusDto::Published),
            strategy: Some("quality".to_string()),
            ..Default::default()
        };
        let (popular, source) = tokio::join!(
            self.analytics.suggestions(&query, 8),
            self.source
                .search_contents(source_query, &query, &excluded_author_ids),
        );
        let mut items = popular;
        if let Ok(source) = source {
            let next_cursor = source.page.next_cursor.clone();
            items.extend(content_suggestions(
                &source.page.items,
                &query,
                &excluded_authors,
            ));
            if let Some(cursor) = next_cursor {
                self.source.release_search_cursor(&cursor).await;
            }
        }
        let lower = query.to_lowercase();
        items.extend(
            self.popular_terms
                .iter()
                .filter(|term| term.to_lowercase().contains(&lower))
                .enumerate()
                .map(|(index, term)| SuggestionDto {
                    text: term.clone(),
                    result_type: SearchResultTypeDto::Topic,
                    score: 0.2 / (index as f64 + 1.0),
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
        Ok(SuggestionResponseDto { query, items })
    }
}

fn make_cursor(
    query: &str,
    search_type: SearchTypeDto,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
    session_id: &str,
) -> String {
    let fingerprint = query_fingerprint(query, search_type, viewer_id, excluded_author_ids);
    format!("v3-{fingerprint:016x}-{session_id}")
}

fn parse_cursor(
    cursor: Option<&str>,
    query: &str,
    search_type: SearchTypeDto,
    viewer_id: Option<&str>,
    excluded_author_ids: &[String],
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
        query_fingerprint(query, search_type, viewer_id, excluded_author_ids)
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
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn content_suggestions(
    contents: &[ContentDto],
    query: &str,
    excluded_authors: &HashSet<String>,
) -> Vec<SuggestionDto> {
    let query = query.to_lowercase();
    let mut items = Vec::new();
    for content in contents
        .iter()
        .filter(|content| !excluded_authors.contains(&content.author_id))
    {
        let base = content.quality_score.clamp(0.0, 1.0);
        push_suggestion(
            &mut items,
            &query,
            &content.post.title,
            if content.content_type == ContentTypeDto::Route {
                SearchResultTypeDto::Journey
            } else {
                SearchResultTypeDto::Post
            },
            1.5 + base,
        );
        push_suggestion(
            &mut items,
            &query,
            &content.post.author_name,
            SearchResultTypeDto::User,
            0.8 + base,
        );
        for topic in content.post.tags.iter().chain(&content.topics) {
            push_suggestion(
                &mut items,
                &query,
                topic,
                SearchResultTypeDto::Topic,
                1.0 + base,
            );
        }
    }
    items
}

fn push_suggestion(
    items: &mut Vec<SuggestionDto>,
    query: &str,
    text: &str,
    result_type: SearchResultTypeDto,
    score: f64,
) {
    let lower = text.to_lowercase();
    if !text.trim().is_empty() && lower.contains(query) {
        items.push(SuggestionDto {
            text: text.to_string(),
            result_type,
            score: score + if lower.starts_with(query) { 1.0 } else { 0.0 },
        });
    }
}

fn deduplicate_suggestions(items: &mut Vec<SuggestionDto>) {
    let mut best = HashMap::<String, SuggestionDto>::new();
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
    contents: &[ContentDto],
    query: &str,
    search_type: SearchTypeDto,
    excluded_authors: &HashSet<String>,
) -> Vec<SearchResultDto> {
    let visible_contents = contents
        .iter()
        .filter(|content| !excluded_authors.contains(&content.author_id))
        .collect::<Vec<_>>();
    match search_type {
        SearchTypeDto::Posts => content_results(&visible_contents, query, true, false),
        SearchTypeDto::Journeys => content_results(&visible_contents, query, false, true),
        SearchTypeDto::Users => user_results(&visible_contents, query),
        SearchTypeDto::Topics => topic_results(&visible_contents, query),
        SearchTypeDto::All => {
            let mut results = content_results(&visible_contents, query, true, true);
            results.extend(user_results(&visible_contents, query));
            results.extend(topic_results(&visible_contents, query));
            results
        }
    }
}

fn sort_results(items: &mut [SearchResultDto]) {
    items.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn result_identity(item: &SearchResultDto) -> String {
    let result_type = match item.result_type {
        SearchResultTypeDto::Post => "post",
        SearchResultTypeDto::Journey => "journey",
        SearchResultTypeDto::User => "user",
        SearchResultTypeDto::Topic => "topic",
    };
    format!("{result_type}:{}", item.id)
}

fn content_results(
    contents: &[&ContentDto],
    query: &str,
    include_posts: bool,
    include_journeys: bool,
) -> Vec<SearchResultDto> {
    contents
        .iter()
        .filter(|content| match content.content_type {
            ContentTypeDto::Route => include_journeys,
            _ => include_posts,
        })
        .filter_map(|content| {
            let fields = [
                content.post.title.as_str(),
                content.post.summary.as_str(),
                content.body.as_str(),
            ];
            let metadata = format!(
                "{} {}",
                content.post.tags.join(" "),
                content.topics.join(" ")
            );
            let (mut score, highlights) = relevance(query, &fields, &metadata)?;
            score += content.quality_score;
            Some(SearchResultDto {
                id: content.id.clone(),
                result_type: if content.content_type == ContentTypeDto::Route {
                    SearchResultTypeDto::Journey
                } else {
                    SearchResultTypeDto::Post
                },
                title: content.post.title.clone(),
                snippet: content.post.summary.clone(),
                cover_url: non_empty(&content.post.cover_url),
                author_id: Some(content.author_id.clone()),
                author_name: Some(content.post.author_name.clone()),
                domain: Some(content.post.domain),
                score,
                highlights,
                post: Some(content.post.clone()),
            })
        })
        .collect()
}

fn user_results(contents: &[&ContentDto], query: &str) -> Vec<SearchResultDto> {
    let mut authors = HashMap::<String, (&PostSummaryDto, usize, f64)>::new();
    for content in contents {
        let entry = authors.entry(content.author_id.clone()).or_insert((
            &content.post,
            0,
            content.quality_score,
        ));
        entry.1 += 1;
        entry.2 = entry.2.max(content.quality_score);
    }
    authors
        .into_iter()
        .filter_map(|(author_id, (post, content_count, quality))| {
            let (score, highlights) = relevance(query, &[post.author_name.as_str()], "")?;
            Some(SearchResultDto {
                id: author_id.clone(),
                result_type: SearchResultTypeDto::User,
                title: post.author_name.clone(),
                snippet: format!("{content_count} 篇公开内容"),
                cover_url: non_empty(&post.author_avatar_url),
                author_id: Some(author_id),
                author_name: Some(post.author_name.clone()),
                domain: None,
                score: score + quality * 0.2,
                highlights,
                post: None,
            })
        })
        .collect()
}

fn topic_results(contents: &[&ContentDto], query: &str) -> Vec<SearchResultDto> {
    let mut topics = HashMap::new();
    for content in contents {
        let content_topics: HashSet<_> = content.post.tags.iter().chain(&content.topics).collect();
        for topic in content_topics {
            let entry = topics.entry(topic.clone()).or_insert((
                0_usize,
                content.quality_score,
                content.post.domain,
            ));
            entry.0 += 1;
            entry.1 = entry.1.max(content.quality_score);
        }
    }
    topics
        .into_iter()
        .filter_map(|(topic, (content_count, quality, domain))| {
            let (score, highlights) = relevance(query, &[topic.as_str()], "")?;
            Some(SearchResultDto {
                id: format!("topic:{topic}"),
                result_type: SearchResultTypeDto::Topic,
                title: topic,
                snippet: format!("{content_count} 条相关内容"),
                cover_url: None,
                author_id: None,
                author_name: None,
                domain: Some(domain),
                score: score + quality * 0.1,
                highlights,
                post: None,
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

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use bookway_api::{ContentMediaDto, ContentPageDto, GrowthDomainDto, SearchTypeDto};

    use super::*;
    use crate::datasource::{
        MemorySearchAnalytics, MemorySearchSessionStore, SearchSessionStore, SearchSourceResult,
    };

    struct StaticSearchSource {
        items: Vec<ContentDto>,
        degraded: bool,
    }

    struct PagedSearchSource {
        items: Vec<ContentDto>,
    }

    #[async_trait]
    impl SearchSource for StaticSearchSource {
        async fn contents(
            &self,
            _query: ContentQueryRequest,
        ) -> Result<SearchSourceResult, SearchSourceError> {
            Ok(SearchSourceResult {
                page: ContentPageDto {
                    items: self.items.clone(),
                    next_cursor: None,
                    total_estimate: self.items.len(),
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
            query: ContentQueryRequest,
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
                .take(limit)
                .cloned()
                .collect::<Vec<_>>();
            let next_offset = offset + items.len();
            Ok(SearchSourceResult {
                page: ContentPageDto {
                    items,
                    next_cursor: (next_offset < self.items.len()).then(|| next_offset.to_string()),
                    total_estimate: self.items.len(),
                },
                degraded: false,
                source_ranked: false,
            })
        }
    }

    #[tokio::test]
    async fn searches_users_and_topics() {
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![content("post-1", "一册", "阅读", "主题阅读")],
            degraded: false,
        }));
        let users = service
            .search(request("一册", SearchTypeDto::Users, None, None))
            .await
            .expect("user search");
        let topics = service
            .search(request("主题", SearchTypeDto::Topics, None, None))
            .await
            .expect("topic search");

        assert_eq!(users.items[0].result_type, SearchResultTypeDto::User);
        assert_eq!(topics.items[0].result_type, SearchResultTypeDto::Topic);
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
            .search(request("阅读", SearchTypeDto::Posts, None, Some(1)))
            .await
            .expect("first page");
        let second = service
            .search(request(
                "阅读",
                SearchTypeDto::Posts,
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
            .search(request("阅读", SearchTypeDto::Posts, None, Some(1)))
            .await
            .expect("first page");

        let error = service
            .search(request(
                "跑步",
                SearchTypeDto::Posts,
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
            .search(request("阅读", SearchTypeDto::Posts, None, Some(1)))
            .await
            .expect("first page");

        let error = service
            .search(request(
                "阅读",
                SearchTypeDto::Users,
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
                .search(request("阅读", SearchTypeDto::Posts, cursor, Some(50)))
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
                SearchTypeDto::Posts,
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
                SearchTypeDto::Users,
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
                SearchTypeDto::Topics,
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
            .suggestions(SuggestionQueryRequest {
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
                SearchTypeDto::Posts,
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
                SearchTypeDto::Posts,
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
                SearchTypeDto::Posts,
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
                SearchTypeDto::Posts,
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
            Arc::new(StaticSearchSource {
                items: vec![
                    content("post-1", "一册", "阅读方法一", "阅读"),
                    content("post-2", "二页", "阅读方法二", "阅读"),
                ],
                degraded: false,
            }),
            Arc::new(MemorySearchAnalytics::default()),
            sessions.clone(),
        );
        let first = service
            .search(request("阅读", SearchTypeDto::Posts, None, Some(1)))
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
            .search(request("阅读", SearchTypeDto::Posts, Some(cursor), Some(1)))
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
            .search(request("阅读", SearchTypeDto::Posts, None, None))
            .await
            .expect("degraded search");

        assert!(response.degraded);
        assert_eq!(response.items.len(), 1);
    }

    #[tokio::test]
    async fn separates_posts_and_journeys_without_misclassifying_all_results() {
        let mut route = content("route-1", "领路人", "阅读入门路线", "阅读");
        route.content_type = ContentTypeDto::Route;
        let service = SearchService::new(Arc::new(StaticSearchSource {
            items: vec![content("post-1", "一册", "阅读笔记", "阅读"), route],
            degraded: false,
        }));

        let posts = service
            .search(request("阅读", SearchTypeDto::Posts, None, None))
            .await
            .expect("post search");
        let all = service
            .search(request("阅读", SearchTypeDto::All, None, None))
            .await
            .expect("all search");

        assert_eq!(posts.items.len(), 1);
        assert_eq!(posts.items[0].id, "post-1");
        assert!(all.items.iter().any(|item| {
            item.id == "route-1" && item.result_type == SearchResultTypeDto::Journey
        }));
    }

    fn request(
        query: &str,
        search_type: SearchTypeDto,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> SearchQueryRequest {
        SearchQueryRequest {
            q: query.to_string(),
            search_type,
            cursor,
            limit,
            user_id: None,
            excluded_author_ids: Vec::new(),
        }
    }

    fn request_with_visibility(
        query: &str,
        search_type: SearchTypeDto,
        cursor: Option<String>,
        limit: Option<usize>,
        viewer_id: &str,
        excluded_author_ids: &[&str],
    ) -> SearchQueryRequest {
        SearchQueryRequest {
            user_id: Some(viewer_id.to_string()),
            excluded_author_ids: excluded_author_ids
                .iter()
                .map(|author_id| (*author_id).to_string())
                .collect(),
            ..request(query, search_type, cursor, limit)
        }
    }

    fn content(id: &str, author: &str, title: &str, topic: &str) -> ContentDto {
        ContentDto {
            id: id.to_string(),
            post: PostSummaryDto {
                id: id.to_string(),
                author_name: author.to_string(),
                author_avatar_url: String::new(),
                title: title.to_string(),
                summary: "把方法用到行动中".to_string(),
                domain: GrowthDomainDto::Learning,
                cover_url: String::new(),
                route_title: String::new(),
                route_duration: String::new(),
                join_count: 0,
                like_count: 0,
                freshness: 1.0,
                tags: vec![topic.to_string()],
            },
            author_id: format!("author-{id}"),
            content_type: ContentTypeDto::Article,
            status: ContentStatusDto::Published,
            body: "正文".to_string(),
            media: Vec::<ContentMediaDto>::new(),
            topics: vec![topic.to_string()],
            created_at: "0".to_string(),
            published_at: Some("0".to_string()),
            version: 1,
            quality_score: 1.0,
        }
    }
}
