use async_trait::async_trait;
use bookway_api::{ApiResponse, ContentPageDto, ContentQueryRequest};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum SearchSourceError {
    #[error("content index source request failed: {0}")]
    Request(#[from] reqwest::Error),
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

pub(crate) struct HttpContentSearchSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpContentSearchSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl SearchSource for HttpContentSearchSource {
    async fn contents(
        &self,
        query: ContentQueryRequest,
    ) -> Result<SearchSourceResult, SearchSourceError> {
        let page = self
            .client
            .get(format!("{}/internal/v1/contents", self.base_url))
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse<ContentPageDto>>()
            .await?
            .data;
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
    fallback: HttpContentSearchSource,
}

impl OpenSearchSource {
    pub(crate) fn new(base_url: String, index: String, fallback_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            index,
            fallback: HttpContentSearchSource::new(fallback_url),
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
        let body = serde_json::json!({
            "from": offset,
            "size": query.limit.unwrap_or(100).clamp(1, 100),
            "track_total_hits": true,
            "query": { "bool": { "must": [{ "multi_match": { "query": text, "fields": ["title^4", "summary^2", "body", "tags", "topics", "author_name"], "type": "best_fields" }}], "filter": [{ "term": { "status": "published" }}] }},
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
