use std::env;

use serde_json::{Value, json};
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
}

impl Config {
    fn from_env() -> Result<Self, RebuildError> {
        let base_url = required_env("OPENSEARCH_URL")?;
        let target_index = required_env("OPENSEARCH_REBUILD_INDEX")?;
        validate_resource_name("OPENSEARCH_REBUILD_INDEX", &target_index)?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            target_index,
            batch_size: env_number("SEARCH_INDEX_REBUILD_BATCH_SIZE", DEFAULT_BATCH_SIZE)?
                .clamp(1, MAX_BATCH_SIZE),
            start_after_id: optional_env("SEARCH_INDEX_REBUILD_AFTER_ID").unwrap_or_default(),
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

    let mut after_id = config.start_after_id.clone();
    let mut stats = RebuildStats::default();
    loop {
        let rows = load_batch(&pool, &after_id, config.batch_size).await?;
        if rows.is_empty() {
            break;
        }
        let last_id = rows.last().map(|row| row.id.clone()).unwrap_or_default();
        let operations = rows
            .into_iter()
            .map(rebuild_operation)
            .collect::<Result<Vec<_>, _>>()?;
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
    if let Some(object) = document.as_object_mut() {
        for (field, value) in fields {
            object.insert(field.to_string(), value);
        }
    }
    document
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
        BulkOperation, ContentRow, bulk_request_body, rebuild_operation, resource_url,
        validate_bulk_response, validate_resource_name,
    };

    #[test]
    fn published_rows_project_nested_search_fields() {
        let operation = rebuild_operation(ContentRow {
            id: "post-1".to_string(),
            version: 4,
            status: "published".to_string(),
            deleted: false,
            payload: json!({ "post": { "title": "A route", "summary": "Start", "author_name": "A", "tags": ["run"] } }),
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
