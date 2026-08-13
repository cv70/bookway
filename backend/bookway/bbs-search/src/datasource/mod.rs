use async_trait::async_trait;
use bookway_api::{ContentPageDto, ContentQueryRequest};
use bookway_bbs_link::api::pb::{self, bbs_link_client::BbsLinkClient};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum SearchSourceError {
    #[error("content index source request failed: {0}")]
    Request(String),
}

pub(crate) struct SearchSourceResult {
    pub(crate) page: ContentPageDto,
    pub(crate) degraded: bool,
}

#[async_trait]
pub(crate) trait SearchSource: Send + Sync {
    async fn contents(
        &self,
        query: ContentQueryRequest,
    ) -> Result<SearchSourceResult, SearchSourceError>;

    async fn search_contents(
        &self,
        query: ContentQueryRequest,
        _text: &str,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        self.contents(query).await
    }
}

pub(crate) struct GrpcContentSearchSource {
    client: BbsLinkClient<tonic::transport::Channel>,
}

impl GrpcContentSearchSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: BbsLinkClient::connect(base_url).await?,
        })
    }
}

#[async_trait]
impl SearchSource for GrpcContentSearchSource {
    async fn contents(
        &self,
        query: ContentQueryRequest,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let mut client = self.client.clone();
        let response = client
            .list(pb::ListRequest {
                request_json: serde_json::to_string(&query)
                    .map_err(|error| SearchSourceError::Request(error.to_string()))?,
            })
            .await
            .map_err(|error| SearchSourceError::Request(error.to_string()))?
            .into_inner();
        let page: ContentPageDto = serde_json::from_str(&response.response_json)
            .map_err(|error| SearchSourceError::Request(error.to_string()))?;
        Ok(SearchSourceResult {
            page,
            degraded: false,
        })
    }
}

pub(crate) struct OpenSearchSource {
    client: reqwest::Client,
    base_url: String,
    index: String,
    fallback: GrpcContentSearchSource,
}

impl OpenSearchSource {
    pub(crate) fn new(base_url: String, index: String, fallback: GrpcContentSearchSource) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            index,
            fallback,
        }
    }
}

#[async_trait]
impl SearchSource for OpenSearchSource {
    async fn contents(
        &self,
        query: ContentQueryRequest,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        self.fallback_contents(query).await
    }

    async fn search_contents(
        &self,
        query: ContentQueryRequest,
        text: &str,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let offset = query
            .cursor
            .as_deref()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut filters = vec![serde_json::json!({ "term": { "status": "published" } })];
        if let Some(content_type) = query.content_type {
            filters.push(
                serde_json::json!({ "term": { "content_type": content_type_name(content_type) } }),
            );
        }
        if let Some(domain) = query.domain {
            filters.push(serde_json::json!({ "term": { "domain": domain_name(domain) } }));
        }
        let body = serde_json::json!({
            "from": offset,
            "size": query.limit.unwrap_or(100).clamp(1, 100),
            "track_total_hits": true,
            "query": { "bool": { "must": [{ "multi_match": { "query": text, "fields": ["title^4", "summary^2", "body", "tags", "topics", "author_name"], "type": "best_fields" }}], "filter": filters }},
            "highlight": { "fields": { "title": {}, "summary": {}, "body": {} } }
        });
        let response = self
            .client
            .post(format!("{}/{}/_search", self.base_url, self.index))
            .json(&body)
            .send()
            .await;
        let response = match response {
            Ok(response) if response.status().is_success() => response,
            Ok(_) | Err(_) => return self.fallback_contents(query).await,
        };
        let payload: serde_json::Value = match response.json().await {
            Ok(payload) => payload,
            Err(_) => return self.fallback_contents(query).await,
        };
        let hits = payload
            .get("hits")
            .and_then(|value| value.get("hits"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let items = hits
            .into_iter()
            .filter_map(|hit| hit.get("_source").cloned())
            .filter_map(|source| serde_json::from_value(source).ok())
            .collect::<Vec<_>>();
        let total = payload
            .get("hits")
            .and_then(|value| value.get("total"))
            .and_then(|value| value.get("value"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(items.len() as u64) as usize;
        Ok(SearchSourceResult {
            page: ContentPageDto {
                next_cursor: (offset + items.len() < total)
                    .then(|| (offset + items.len()).to_string()),
                total_estimate: total,
                items,
            },
            degraded: false,
        })
    }
}

fn content_type_name(value: bookway_api::ContentTypeDto) -> &'static str {
    match value {
        bookway_api::ContentTypeDto::Note => "note",
        bookway_api::ContentTypeDto::Article => "article",
        bookway_api::ContentTypeDto::Video => "video",
        bookway_api::ContentTypeDto::Route => "route",
    }
}

fn domain_name(value: bookway_api::GrowthDomainDto) -> &'static str {
    match value {
        bookway_api::GrowthDomainDto::Learning => "learning",
        bookway_api::GrowthDomainDto::Movement => "movement",
        bookway_api::GrowthDomainDto::Wellness => "wellness",
        bookway_api::GrowthDomainDto::Travel => "travel",
        bookway_api::GrowthDomainDto::Leisure => "leisure",
    }
}

impl OpenSearchSource {
    async fn fallback_contents(
        &self,
        query: ContentQueryRequest,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let mut result = self.fallback.contents(query).await?;
        result.degraded = true;
        Ok(result)
    }
}
