use super::*;

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
