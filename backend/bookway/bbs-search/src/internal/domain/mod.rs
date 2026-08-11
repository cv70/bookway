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
    api::{SearchQueryRequest, SearchResponseDto, SuggestionResponseDto},
    datasource::{SearchSource, SearchSourceError},
};

const MAX_SEARCH_CANDIDATES: usize = 100;
const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;

#[derive(Debug, Error)]
pub(crate) enum SearchError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Source(#[from] SearchSourceError),
}

#[derive(Clone)]
pub(crate) struct SearchService {
    source: Arc<dyn SearchSource>,
    popular_terms: Arc<Vec<String>>,
}

impl SearchService {
    pub(crate) fn new(source: Arc<dyn SearchSource>) -> Self {
        Self {
            source,
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
        let offset = parse_cursor(request.cursor.as_deref())?;
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_PAGE_SIZE);
        let started = Instant::now();
        let source_result = self
            .source
            .search_contents(
                ContentQueryRequest {
                    cursor: None,
                    limit: Some(MAX_SEARCH_CANDIDATES),
                    status: Some(ContentStatusDto::Published),
                    strategy: Some("fresh".to_string()),
                    ids: None,
                },
                &query_text,
            )
            .await?;
        let content_page = source_result.page;

        let mut items = match request.search_type {
            SearchTypeDto::Posts => post_results(&content_page.items, &query_text, false),
            SearchTypeDto::Journeys => post_results(&content_page.items, &query_text, true),
            SearchTypeDto::Users => user_results(&content_page.items, &query_text),
            SearchTypeDto::Topics => topic_results(&content_page.items, &query_text),
            SearchTypeDto::All => {
                let mut results = post_results(&content_page.items, &query_text, false);
                results.extend(user_results(&content_page.items, &query_text));
                results.extend(topic_results(&content_page.items, &query_text));
                results
            }
        };
        items.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });

        let total_estimate = items.len();
        let page: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
        let next_cursor =
            (offset + page.len() < total_estimate).then(|| (offset + page.len()).to_string());
        Ok(SearchResponseDto {
            query: query_text,
            items: page,
            next_cursor,
            total_estimate,
            took_ms: started.elapsed().as_millis() as u64,
            degraded: source_result.degraded,
        })
    }

    pub(crate) async fn suggestions(
        &self,
        query: &str,
    ) -> Result<SuggestionResponseDto, SearchError> {
        let query = query.trim().to_string();
        if query.is_empty() {
            return Ok(SuggestionResponseDto {
                query,
                items: Vec::new(),
            });
        }
        let lower = query.to_lowercase();
        let items = self
            .popular_terms
            .iter()
            .filter(|term| term.to_lowercase().contains(&lower))
            .enumerate()
            .map(|(index, term)| SuggestionDto {
                text: term.clone(),
                result_type: SearchResultTypeDto::Topic,
                score: 1.0 / (index as f64 + 1.0),
            })
            .take(8)
            .collect();
        Ok(SuggestionResponseDto { query, items })
    }
}

fn parse_cursor(cursor: Option<&str>) -> Result<usize, SearchError> {
    cursor
        .map(str::parse)
        .transpose()
        .map(|offset| offset.unwrap_or(0))
        .map_err(|_| SearchError::Validation("搜索游标无效".to_string()))
}

fn post_results(contents: &[ContentDto], query: &str, journeys_only: bool) -> Vec<SearchResultDto> {
    contents
        .iter()
        .filter(|content| !journeys_only || content.content_type == ContentTypeDto::Route)
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
                result_type: if journeys_only {
                    SearchResultTypeDto::Journey
                } else {
                    SearchResultTypeDto::Post
                },
                title: content.post.title.clone(),
                snippet: content.post.summary.clone(),
                cover_url: non_empty(&content.post.cover_url),
                author_name: Some(content.post.author_name.clone()),
                domain: Some(content.post.domain),
                score,
                highlights,
                post: Some(content.post.clone()),
            })
        })
        .collect()
}

fn user_results(contents: &[ContentDto], query: &str) -> Vec<SearchResultDto> {
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
                id: author_id,
                result_type: SearchResultTypeDto::User,
                title: post.author_name.clone(),
                snippet: format!("{content_count} 篇公开内容"),
                cover_url: non_empty(&post.author_avatar_url),
                author_name: Some(post.author_name.clone()),
                domain: None,
                score: score + quality * 0.2,
                highlights,
                post: None,
            })
        })
        .collect()
}

fn topic_results(contents: &[ContentDto], query: &str) -> Vec<SearchResultDto> {
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
    use crate::internal::datasource::SearchSourceResult;

    struct StaticSearchSource {
        items: Vec<ContentDto>,
        degraded: bool,
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
        assert_eq!(first.next_cursor.as_deref(), Some("1"));
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].id, second.items[0].id);
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
