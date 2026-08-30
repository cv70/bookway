use std::env;

use bookway_knowledge_catalog_api::pb::{
    EmbedTextsRequest, knowledge_catalog_client::KnowledgeCatalogClient,
};
use serde_json::{Value, json};
use std::time::Duration;
use thiserror::Error;

const DEFAULT_BATCH_SIZE: i64 = 500;
const MAX_BATCH_SIZE: i64 = 2_000;

#[derive(Debug, Error)]
enum RebuildError {
    #[error("{key} is required")]
    MissingEnvironment { key: &'static str },
    #[error("invalid {key}: {value}")]
    InvalidEnvironment { key: &'static str, value: String },
    #[error("OpenSearch request failed: {0}")]
    Request(String),
    #[error("OpenSearch rebuild index is missing: {0}")]
    TargetMissing(String),
    #[error("OpenSearch rebuild target must resolve to one concrete index: {0}")]
    TargetNotConcrete(String),
    #[error("OpenSearch bulk response is invalid: {0}")]
    BulkResponse(String),
    #[error("content {0} has an invalid version")]
    InvalidContentVersion(String),
}

#[derive(Debug)]
struct Config {
    base_url: String,
    target_index: String,
    batch_size: i64,
    start_after_id: String,
    semantic_dims: Option<usize>,
    knowledge_catalog_url: String,
}

impl Config {
    fn from_env() -> Result<Self, RebuildError> {
        let base_url = required_env("OPENSEARCH_URL")?;
        let target_index = required_env("OPENSEARCH_REBUILD_INDEX")?;
        validate_resource_name("OPENSEARCH_REBUILD_INDEX", &target_index)?;
        let semantic_dims_value = optional_env("SEMANTIC_VECTOR_DIMS");
        let semantic_dims = semantic_dims_value
            .clone()
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| RebuildError::InvalidEnvironment {
                        key: "SEMANTIC_VECTOR_DIMS",
                        value,
                    })
            })
            .transpose()?
            .filter(|dims| (8..=4096).contains(dims));
        if semantic_dims_value.is_some() && semantic_dims.is_none() {
            return Err(RebuildError::InvalidEnvironment {
                key: "SEMANTIC_VECTOR_DIMS",
                value: semantic_dims_value.unwrap_or_default(),
            });
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            target_index,
            batch_size: env_number("SEARCH_INDEX_REBUILD_BATCH_SIZE", DEFAULT_BATCH_SIZE)?
                .clamp(1, MAX_BATCH_SIZE),
            start_after_id: optional_env("SEARCH_INDEX_REBUILD_AFTER_ID").unwrap_or_default(),
            semantic_dims,
            knowledge_catalog_url: optional_env("KNOWLEDGE_CATALOG_GRPC_URL")
                .unwrap_or_else(|| "http://127.0.0.1:8105".to_string()),
        })
    }
}

fn required_env(key: &'static str) -> Result<String, RebuildError> {
    optional_env(key).ok_or(RebuildError::MissingEnvironment { key })
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_number(key: &'static str, default: i64) -> Result<i64, RebuildError> {
    match optional_env(key) {
        Some(value) => value
            .parse()
            .map_err(|_| RebuildError::InvalidEnvironment { key, value }),
        None => Ok(default),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ContentRow {
    id: String,
    version: i64,
    payload: Value,
    status: String,
    deleted: bool,
}

#[derive(Debug)]
enum BulkOperation {
    Upsert {
        id: String,
        version: i64,
        document: Value,
    },
    Delete {
        id: String,
        version: i64,
    },
}

#[derive(Default)]
struct RebuildStats {
    batches: u64,
    upserts: u64,
    deletes: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-index-rebuild");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let client = bookway_runtime::http_client();
    validate_target(&client, &config).await?;
    if let Some(dims) = config.semantic_dims {
        ensure_semantic_mapping(&client, &config.base_url, &config.target_index, dims).await?;
    }
    let catalog = match config.semantic_dims {
        Some(dims) => match bookway_runtime::grpc_channel(&config.knowledge_catalog_url).await {
            Ok(channel) => Some((dims, KnowledgeCatalogClient::new(channel))),
            Err(error) => {
                tracing::warn!(%error, "knowledge-catalog unavailable; rebuilding lexical documents only");
                None
            }
        },
        None => None,
    };

    let mut after_id = config.start_after_id.clone();
    let mut stats = RebuildStats::default();
    loop {
        let rows = load_batch(&pool, &after_id, config.batch_size).await?;
        if rows.is_empty() {
            break;
        }
        let last_id = rows.last().map(|row| row.id.clone()).unwrap_or_default();
        let mut operations = rows
            .into_iter()
            .map(rebuild_operation)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some((dims, catalog)) = catalog.as_ref() {
            for operation in &mut operations {
                if let BulkOperation::Upsert { document, .. } = operation {
                    embed_document(catalog, document, *dims).await;
                }
            }
        }
        let (upserts, deletes) = operation_counts(&operations);
        submit_bulk(&client, &config, &operations).await?;
        stats.batches = stats.batches.saturating_add(1);
        stats.upserts = stats.upserts.saturating_add(upserts);
        stats.deletes = stats.deletes.saturating_add(deletes);
        after_id = last_id;
        tracing::info!(
            target_index = %config.target_index,
            batch = stats.batches,
            last_content_id = %after_id,
            upserts,
            deletes,
            "search index rebuild batch completed"
        );
    }
    refresh_target(&client, &config).await?;

    println!(
        "{}",
        json!({
            "status": "completed",
            "target_index": config.target_index,
            "start_after_id": config.start_after_id,
            "next_after_id": after_id,
            "batches": stats.batches,
            "upserts": stats.upserts,
            "deletes": stats.deletes,
        })
    );
    Ok(())
}

async fn load_batch(
    pool: &sqlx::PgPool,
    after_id: &str,
    batch_size: i64,
) -> Result<Vec<ContentRow>, sqlx::Error> {
    // Keyset pagination is restart-safe: replayed pages carry the same source
    // version and OpenSearch accepts equal external versions idempotently.
    sqlx::query_as::<_, ContentRow>(
        "SELECT id, version, payload, status, deleted_at IS NOT NULL AS deleted FROM content_items WHERE id > $1 ORDER BY id LIMIT $2",
    )
    .bind(after_id)
    .bind(batch_size)
    .fetch_all(pool)
    .await
}

fn rebuild_operation(row: ContentRow) -> Result<BulkOperation, RebuildError> {
    if row.version <= 0 {
        return Err(RebuildError::InvalidContentVersion(row.id));
    }
    if row.deleted || row.status != "published" {
        return Ok(BulkOperation::Delete {
            id: row.id,
            version: row.version,
        });
    }
    Ok(BulkOperation::Upsert {
        id: row.id,
        version: row.version,
        document: index_document(row.payload),
    })
}

fn index_document(mut document: Value) -> Value {
    // Post summaries are nested in the content contract, while the search
    // mapping stores the queried fields at the root.
    let fields = document
        .get("post")
        .and_then(Value::as_object)
        .map(|post| {
            ["title", "summary", "author_name", "tags"]
                .into_iter()
                .filter_map(|field| post.get(field).cloned().map(|value| (field, value)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content_type = document.get("content_type").and_then(content_type_name);
    let domain = document
        .get("post")
        .and_then(Value::as_object)
        .and_then(|post| post.get("domain"))
        .and_then(growth_domain_name);
    let (route_action_ids, route_action_titles, route_action_details, route_scene_equipment) =
        route_action_search_fields(&document);
    if let Some(object) = document.as_object_mut() {
        for (field, value) in fields {
            object.insert(field.to_string(), value);
        }
        // Keep rebuilds byte-for-byte compatible with the live indexer
        // projection. Alias switching must not remove typed node/equipment
        // search fields or leave enum filters encoded as protobuf integers.
        object.insert("status".to_string(), Value::String("published".to_string()));
        if let Some(content_type) = content_type {
            object.insert(
                "content_type".to_string(),
                Value::String(content_type.to_string()),
            );
        }
        if let Some(domain) = domain {
            object.insert("domain".to_string(), Value::String(domain.to_string()));
        }
        object.insert("route_action_ids".to_string(), json!(route_action_ids));
        object.insert(
            "route_action_titles".to_string(),
            json!(route_action_titles),
        );
        object.insert(
            "route_action_details".to_string(),
            json!(route_action_details),
        );
        object.insert(
            "route_scene_equipment".to_string(),
            json!(route_scene_equipment),
        );
    }
    document
}

fn route_action_search_fields(
    document: &Value,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let actions = document
        .get("route_template")
        .and_then(Value::as_object)
        .and_then(|template| template.get("actions"))
        .and_then(Value::as_array);
    let mut ids = Vec::new();
    let mut titles = Vec::new();
    let mut details = Vec::new();
    let mut equipment = Vec::new();
    for action in actions.into_iter().flatten().filter_map(Value::as_object) {
        append_non_empty(&mut ids, action.get("id"));
        append_non_empty(&mut titles, action.get("title"));
        append_non_empty(&mut details, action.get("detail"));
        append_non_empty(&mut details, action.get("scheduled_label"));
        for value in action
            .get("scene_equipment")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            append_non_empty(&mut equipment, Some(value));
        }
    }
    (ids, titles, details, equipment)
}

fn append_non_empty(values: &mut Vec<String>, value: Option<&Value>) {
    if let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.push(value.to_string());
    }
}

fn content_type_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Number(value) => match value.as_i64() {
            Some(0) => Some("note"),
            Some(1) => Some("article"),
            Some(2) => Some("video"),
            Some(3) => Some("route"),
            Some(4) => Some("milestone"),
            Some(5) => Some("question"),
            _ => None,
        },
        _ => None,
    }
}

fn growth_domain_name(value: &Value) -> Option<&'static str> {
    match value {
        Value::Number(value) => match value.as_i64() {
            Some(0) => Some("learning"),
            Some(1) => Some("movement"),
            Some(2) => Some("wellness"),
            Some(3) => Some("travel"),
            Some(4) => Some("leisure"),
            _ => None,
        },
        _ => None,
    }
}

/// Rebuilds can be the first writer for a fresh semantic index. Populate the
/// vector from the same node-aware text as the live indexer; failures leave a
/// valid lexical document in place and can be retried by a later rebuild.
async fn embed_document(
    catalog: &KnowledgeCatalogClient<tonic::transport::Channel>,
    document: &mut Value,
    dims: usize,
) {
    let text = semantic_text(document);
    if text.is_empty() {
        return;
    }
    let request =
        match bookway_runtime::grpc_service_request(EmbedTextsRequest { texts: vec![text] }) {
            Ok(request) => request,
            Err(error) => {
                tracing::debug!(%error, "semantic embed skipped during rebuild");
                return;
            }
        };
    let mut client = catalog.clone();
    match tokio::time::timeout(Duration::from_secs(5), client.embed_texts(request)).await {
        Ok(Ok(response)) => {
            if let Some(embedding) = response.into_inner().embeddings.first()
                && embedding.values.len() == dims
                && embedding.values.iter().all(|value| value.is_finite())
                && embedding.values.iter().any(|value| *value != 0.0)
                && let Some(object) = document.as_object_mut()
            {
                object.insert("semantic_vector".to_string(), json!(embedding.values));
            }
        }
        Ok(Err(error)) => tracing::debug!(%error, "semantic embed degraded during rebuild"),
        Err(_) => tracing::debug!("semantic embed timed out during rebuild"),
    }
}

fn semantic_text(document: &Value) -> String {
    let post = document.get("post").and_then(Value::as_object);
    let mut parts = Vec::new();
    for field in ["title", "summary"] {
        if let Some(value) = post
            .and_then(|post| post.get(field))
            .and_then(Value::as_str)
        {
            parts.push(value.to_string());
        }
    }
    let (_, titles, _, equipment) = route_action_search_fields(document);
    parts.extend(titles);
    parts.extend(equipment);
    parts.join(" ")
}

async fn ensure_semantic_mapping(
    client: &reqwest::Client,
    base_url: &str,
    index: &str,
    dims: usize,
) -> Result<(), RebuildError> {
    let response = client
        .put(resource_url(base_url, &[index, "_mapping"])?)
        .json(&json!({
            "properties": {
                "semantic_vector": {
                    "type": "knn_vector",
                    "dimension": dims,
                    "method": { "name": "hnsw", "space_type": "cosinesimil", "engine": "lucene" }
                }
            }
        }))
        .send()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(RebuildError::Request(format!(
            "semantic vector mapping rejected by OpenSearch: {}",
            response.status()
        )))
    }
}

fn operation_counts(operations: &[BulkOperation]) -> (u64, u64) {
    operations
        .iter()
        .fold((0, 0), |(upserts, deletes), operation| match operation {
            BulkOperation::Upsert { .. } => (upserts + 1, deletes),
            BulkOperation::Delete { .. } => (upserts, deletes + 1),
        })
}

async fn validate_target(client: &reqwest::Client, config: &Config) -> Result<(), RebuildError> {
    let target_url = resource_url(&config.base_url, &[&config.target_index])?;
    let exists = client
        .head(target_url)
        .send()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if exists.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(RebuildError::TargetMissing(config.target_index.clone()));
    }
    if !exists.status().is_success() {
        return Err(RebuildError::Request(format!(
            "rebuild target check returned {}",
            exists.status()
        )));
    }

    let resolved = client
        .get(resource_url(
            &config.base_url,
            &["_resolve", "index", &config.target_index],
        )?)
        .send()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if !resolved.status().is_success() {
        return Err(RebuildError::Request(format!(
            "rebuild target resolution returned {}",
            resolved.status()
        )));
    }
    let payload = resolved
        .json::<Value>()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if resolves_to_concrete_index(&payload, &config.target_index) {
        Ok(())
    } else {
        Err(RebuildError::TargetNotConcrete(config.target_index.clone()))
    }
}

async fn submit_bulk(
    client: &reqwest::Client,
    config: &Config,
    operations: &[BulkOperation],
) -> Result<(), RebuildError> {
    let body = bulk_request_body(operations)?;
    let response = client
        .post(resource_url(
            &config.base_url,
            &[&config.target_index, "_bulk"],
        )?)
        .header(reqwest::header::CONTENT_TYPE, "application/x-ndjson")
        .body(body)
        .send()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if !response.status().is_success() {
        return Err(RebuildError::Request(format!(
            "bulk rebuild returned {}",
            response.status()
        )));
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    validate_bulk_response(&payload, operations.len())
}

async fn refresh_target(client: &reqwest::Client, config: &Config) -> Result<(), RebuildError> {
    let response = client
        .post(resource_url(
            &config.base_url,
            &[&config.target_index, "_refresh"],
        )?)
        .send()
        .await
        .map_err(|error| RebuildError::Request(error.to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(RebuildError::Request(format!(
            "rebuild target refresh returned {}",
            response.status()
        )))
    }
}

fn bulk_request_body(operations: &[BulkOperation]) -> Result<String, RebuildError> {
    let mut body = String::new();
    for operation in operations {
        match operation {
            BulkOperation::Upsert {
                id,
                version,
                document,
            } => {
                append_bulk_line(
                    &mut body,
                    &json!({ "index": { "_id": id, "version": version, "version_type": "external_gte" } }),
                )?;
                append_bulk_line(&mut body, document)?;
            }
            BulkOperation::Delete { id, version } => append_bulk_line(
                &mut body,
                &json!({ "delete": { "_id": id, "version": version, "version_type": "external_gte" } }),
            )?,
        }
    }
    Ok(body)
}

fn append_bulk_line(body: &mut String, value: &Value) -> Result<(), RebuildError> {
    let line = serde_json::to_string(value)
        .map_err(|error| RebuildError::BulkResponse(error.to_string()))?;
    body.push_str(&line);
    body.push('\n');
    Ok(())
}

fn validate_bulk_response(payload: &Value, expected_items: usize) -> Result<(), RebuildError> {
    let items = payload
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| RebuildError::BulkResponse("items are missing".to_string()))?;
    if items.len() != expected_items {
        return Err(RebuildError::BulkResponse(format!(
            "expected {expected_items} bulk items but received {}",
            items.len()
        )));
    }
    for item in items {
        let Some(entry) = item.as_object().and_then(|actions| {
            actions
                .get("index")
                .map(|value| ("index", value))
                .or_else(|| actions.get("delete").map(|value| ("delete", value)))
        }) else {
            return Err(RebuildError::BulkResponse(
                "item has no index or delete action".to_string(),
            ));
        };
        let status = entry
            .1
            .get("status")
            .and_then(Value::as_u64)
            .ok_or_else(|| RebuildError::BulkResponse("item status is missing".to_string()))?;
        let delete_not_found = entry.0 == "delete" && status == 404;
        let stale_version = status == 409
            && entry
                .1
                .get("error")
                .and_then(|error| error.get("type"))
                .and_then(Value::as_str)
                == Some("version_conflict_engine_exception");
        if !(200..300).contains(&status) && !delete_not_found && !stale_version {
            return Err(RebuildError::BulkResponse(format!(
                "{} action failed with status {status}",
                entry.0
            )));
        }
    }
    Ok(())
}

fn resolves_to_concrete_index(payload: &Value, target_index: &str) -> bool {
    let has_exact_index = payload
        .get("indices")
        .and_then(Value::as_array)
        .is_some_and(|indices| {
            indices
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    let has_same_named_alias = payload
        .get("aliases")
        .and_then(Value::as_array)
        .is_some_and(|aliases| {
            aliases
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    let has_same_named_data_stream = payload
        .get("data_streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    has_exact_index && !has_same_named_alias && !has_same_named_data_stream
}

fn validate_resource_name(key: &'static str, value: &str) -> Result<(), RebuildError> {
    let bytes = value.as_bytes();
    let valid_start = matches!(bytes.first(), Some(b'a'..=b'z' | b'0'..=b'9'));
    let valid_characters = bytes.iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(*byte, b'-' | b'_' | b'.')
    });
    if value.len() > 255
        || !valid_start
        || !valid_characters
        || matches!(value, "." | "..")
        || value.contains("..")
    {
        return Err(RebuildError::InvalidEnvironment {
            key,
            value: value.to_string(),
        });
    }
    Ok(())
}

fn resource_url(base_url: &str, path: &[&str]) -> Result<reqwest::Url, RebuildError> {
    let mut url =
        reqwest::Url::parse(base_url).map_err(|error| RebuildError::InvalidEnvironment {
            key: "OPENSEARCH_URL",
            value: error.to_string(),
        })?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| RebuildError::InvalidEnvironment {
            key: "OPENSEARCH_URL",
            value: "cannot be used as a base URL".to_string(),
        })?;
    segments.pop_if_empty();
    for segment in path {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        BulkOperation, ContentRow, bulk_request_body, index_document, rebuild_operation,
        resource_url, semantic_text, validate_bulk_response, validate_resource_name,
    };

    #[test]
    fn published_rows_project_nested_search_fields() {
        let operation = rebuild_operation(ContentRow {
            id: "post-1".to_string(),
            version: 4,
            status: "published".to_string(),
            deleted: false,
            payload: json!({
                "content_type": 3,
                "post": {
                    "title": "A route",
                    "summary": "Start",
                    "author_name": "A",
                    "domain": 1,
                    "tags": ["run"]
                },
                "route_template": {
                    "actions": [{
                        "id": "node-1",
                        "title": "Warm up",
                        "detail": "Walk ten minutes",
                        "scheduled_label": "Today",
                        "scene_equipment": ["shoes"]
                    }]
                }
            }),
        })
        .expect("valid row");
        let BulkOperation::Upsert {
            document, version, ..
        } = operation
        else {
            panic!("published row should upsert");
        };
        assert_eq!(version, 4);
        assert_eq!(document["title"], "A route");
        assert_eq!(document["tags"], json!(["run"]));
        assert_eq!(document["status"], "published");
        assert_eq!(document["content_type"], "route");
        assert_eq!(document["domain"], "movement");
        assert_eq!(document["route_action_ids"], json!(["node-1"]));
        assert_eq!(document["route_action_titles"], json!(["Warm up"]));
        assert_eq!(
            document["route_action_details"],
            json!(["Walk ten minutes", "Today"])
        );
        assert_eq!(document["route_scene_equipment"], json!(["shoes"]));
    }

    #[test]
    fn unpublished_and_deleted_rows_delete_their_projection() {
        for (status, deleted) in [("reviewing", false), ("published", true)] {
            let operation = rebuild_operation(ContentRow {
                id: "post-1".to_string(),
                version: 2,
                payload: json!({}),
                status: status.to_string(),
                deleted,
            })
            .expect("valid row");
            assert!(matches!(operation, BulkOperation::Delete { .. }));
        }
    }

    #[test]
    fn semantic_projection_contains_route_nodes_and_equipment() {
        let document = index_document(json!({
            "post": { "title": "morning training", "summary": "start small" },
            "route_template": {
                "actions": [{
                    "title": "warmup",
                    "scene_equipment": ["yoga mat"]
                }]
            }
        }));
        let text = semantic_text(&document);
        assert!(text.contains("morning training"));
        assert!(text.contains("warmup"));
        assert!(text.contains("yoga mat"));
    }

    #[test]
    fn bulk_body_carries_external_versions_and_trailing_newline() {
        let body = bulk_request_body(&[
            BulkOperation::Upsert {
                id: "post-1".to_string(),
                version: 7,
                document: json!({ "title": "A route" }),
            },
            BulkOperation::Delete {
                id: "post-2".to_string(),
                version: 8,
            },
        ])
        .expect("serializes");
        assert!(body.ends_with('\n'));
        let lines = body.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            serde_json::from_str::<Value>(lines[0]).expect("metadata"),
            json!({ "index": { "_id": "post-1", "version": 7, "version_type": "external_gte" } })
        );
        assert_eq!(
            serde_json::from_str::<Value>(lines[2]).expect("metadata"),
            json!({ "delete": { "_id": "post-2", "version": 8, "version_type": "external_gte" } })
        );
    }

    #[test]
    fn bulk_response_accepts_only_safe_idempotent_conflicts() {
        assert!(validate_bulk_response(&json!({
            "items": [
                { "index": { "status": 201 } },
                { "delete": { "status": 404 } },
                { "index": { "status": 409, "error": { "type": "version_conflict_engine_exception" } } }
            ]
        }), 3)
        .is_ok());
        assert!(validate_bulk_response(&json!({
            "items": [{ "index": { "status": 400, "error": { "type": "mapper_parsing_exception" } } }]
        }), 1)
        .is_err());
        assert!(validate_bulk_response(&json!({ "items": [] }), 1).is_err());
    }

    #[test]
    fn index_name_and_url_paths_are_strict() {
        assert!(validate_resource_name("key", "bookway-content-v2").is_ok());
        assert!(validate_resource_name("key", ".system").is_err());
        let url = resource_url("https://search.example/api", &["index", "a/b"]).expect("valid URL");
        assert_eq!(url.as_str(), "https://search.example/api/index/a%2Fb");
    }
}
