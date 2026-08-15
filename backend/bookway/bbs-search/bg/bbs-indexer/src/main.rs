use std::{env, time::Duration};

use serde_json::Value;
use uuid::Uuid;

const INDEXER_BATCH_SIZE: i64 = 500;
const JOB_LEASE_SECONDS: i32 = 300;
const MAX_ATTEMPTS: i32 = 10;
const DEFAULT_WRITE_INDEX: &str = "bookway-content-v1";

#[derive(Debug)]
struct IndexJob {
    content_id: String,
    content_version: i64,
    lease_id: Uuid,
}

enum IndexOperation {
    Upsert(Value),
    Delete,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-indexer");
    let pool = bookway_data::postgres_pool().await?;
    let client = bookway_runtime::http_client();
    let url = env::var("OPENSEARCH_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9200".to_string())
        .trim_end_matches('/')
        .to_string();
    let (write_indices, using_legacy_index) = configured_write_indices()?;
    if using_legacy_index {
        tracing::warn!(
            write_index = %write_indices[0],
            "OPENSEARCH_WRITE_INDEX is not configured; using deprecated OPENSEARCH_INDEX fallback"
        );
    }
    for index in &write_indices {
        ensure_index(&client, &url, index).await?;
    }

    loop {
        let jobs = match claim_jobs(&pool).await {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::error!(%error, "could not claim content index jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if jobs.is_empty() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        for job in jobs {
            if let Err(error) = synchronize_job(&client, &pool, &url, &write_indices, &job).await {
                tracing::warn!(content_id = %job.content_id, version = job.content_version, %error, "content index job failed");
                match schedule_retry(&pool, &job, &error).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::debug!(content_id = %job.content_id, "content index lease was replaced before retry")
                    }
                    Err(retry_error) => {
                        tracing::error!(content_id = %job.content_id, %retry_error, "could not schedule content index retry")
                    }
                }
            }
        }
    }
}

fn configured_write_indices() -> Result<(Vec<String>, bool), String> {
    let (primary, using_legacy_index) = if let Some(index) = non_empty_env("OPENSEARCH_WRITE_INDEX")
    {
        (index, false)
    } else {
        (
            non_empty_env("OPENSEARCH_INDEX").unwrap_or_else(|| DEFAULT_WRITE_INDEX.to_string()),
            true,
        )
    };
    write_indices(primary, non_empty_env("OPENSEARCH_SHADOW_WRITE_INDEX"))
        .map(|indices| (indices, using_legacy_index))
}

fn write_indices(primary: String, shadow: Option<String>) -> Result<Vec<String>, String> {
    let mut indices = vec![primary];
    if let Some(shadow) = shadow {
        if shadow == indices[0] {
            return Err(
                "OPENSEARCH_SHADOW_WRITE_INDEX must differ from OPENSEARCH_WRITE_INDEX".to_string(),
            );
        }
        indices.push(shadow);
    }
    Ok(indices)
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn claim_jobs(pool: &sqlx::PgPool) -> Result<Vec<IndexJob>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // `lease_id` prevents a slow worker from acknowledging a job that a
    // replacement worker reclaimed after the lease expired.
    let rows = sqlx::query_as::<_, (String, i64, Uuid)>(
        r#"
        WITH claimed AS (
            SELECT content_id
            FROM content_index_outbox
            WHERE (status = 'pending' AND available_at <= now())
               OR (
                    status = 'processing'
                    AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2)
               )
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE content_index_outbox AS job
        SET status = 'processing',
            attempts = job.attempts + 1,
            locked_at = now(),
            lease_id = gen_random_uuid(),
            updated_at = now()
        FROM claimed
        WHERE job.content_id = claimed.content_id
        RETURNING job.content_id, job.content_version, job.lease_id
        "#,
    )
    .bind(INDEXER_BATCH_SIZE)
    .bind(JOB_LEASE_SECONDS)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(content_id, content_version, lease_id)| IndexJob {
            content_id,
            content_version,
            lease_id,
        })
        .collect())
}

async fn synchronize_job(
    client: &reqwest::Client,
    pool: &sqlx::PgPool,
    base_url: &str,
    indices: &[String],
    job: &IndexJob,
) -> Result<(), String> {
    let content = sqlx::query_as::<_, (Value, String, i64, bool)>(
        "SELECT payload, status, version, deleted_at IS NOT NULL FROM content_items WHERE id = $1",
    )
    .bind(&job.content_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (operation, document_version) = content.map_or(
        (IndexOperation::Delete, job.content_version),
        |(payload, status, version, deleted)| {
            let operation = if deleted {
                IndexOperation::Delete
            } else {
                index_operation(payload, &status)
            };
            (operation, version)
        },
    );
    synchronize_operation(
        client,
        base_url,
        indices,
        &job.content_id,
        document_version,
        &operation,
    )
    .await?;
    mark_delivered(pool, job).await
}

async fn synchronize_operation(
    client: &reqwest::Client,
    base_url: &str,
    indices: &[String],
    content_id: &str,
    document_version: i64,
    operation: &IndexOperation,
) -> Result<(), String> {
    if document_version <= 0 {
        return Err(format!(
            "content {content_id} has an invalid search version"
        ));
    }
    for index in indices {
        let document_url = versioned_document_url(base_url, index, content_id, document_version)?;
        let response = match operation {
            IndexOperation::Upsert(document) => {
                client.put(document_url).json(document).send().await
            }
            IndexOperation::Delete => client.delete(document_url).send().await,
        }
        .map_err(|error| error.to_string())?;
        if !response.status().is_success() && response.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(format!(
                "OpenSearch write to {index} returned {}",
                response.status()
            ));
        }
    }
    Ok(())
}

async fn mark_delivered(pool: &sqlx::PgPool, job: &IndexJob) -> Result<(), String> {
    // A content mutation can arrive while this lease is active. In that case
    // leave the latest revision pending for a second pass rather than marking
    // an older projection as complete.
    let updated = sqlx::query(
        r#"
        UPDATE content_index_outbox
        SET status = CASE
                WHEN content_version = $3 THEN 'delivered'
                ELSE 'pending'
            END,
            attempts = CASE
                WHEN content_version = $3 THEN attempts
                ELSE 0
            END,
            available_at = now(),
            locked_at = NULL,
            lease_id = NULL,
            last_error = NULL,
            updated_at = now()
        WHERE content_id = $1 AND lease_id = $2 AND status = 'processing'
        "#,
    )
    .bind(&job.content_id)
    .bind(job.lease_id)
    .bind(job.content_version)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() == 1 {
        Ok(())
    } else {
        Err("content index lease was replaced before completion".to_string())
    }
}

async fn schedule_retry(
    pool: &sqlx::PgPool,
    job: &IndexJob,
    error: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        r#"
        UPDATE content_index_outbox
        SET status = CASE
                WHEN content_version <> $3 THEN 'pending'
                WHEN attempts >= $4 THEN 'dead'
                ELSE 'pending'
            END,
            attempts = CASE
                WHEN content_version <> $3 THEN 0
                ELSE attempts
            END,
            available_at = CASE
                WHEN content_version <> $3 THEN now()
                WHEN attempts >= $4 THEN now()
                ELSE now() + make_interval(
                    secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))
                )
            END,
            locked_at = NULL,
            lease_id = NULL,
            last_error = left($5, 2000),
            updated_at = now()
        WHERE content_id = $1 AND lease_id = $2 AND status = 'processing'
        "#,
    )
    .bind(&job.content_id)
    .bind(job.lease_id)
    .bind(job.content_version)
    .bind(MAX_ATTEMPTS)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

fn index_operation(mut document: Value, status: &str) -> IndexOperation {
    if status != "published" {
        return IndexOperation::Delete;
    }
    // Post summaries are nested in the content contract, while the search
    // index queries these fields at the root for efficient multi-match reads.
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
    IndexOperation::Upsert(document)
}

async fn ensure_index(client: &reqwest::Client, base_url: &str, index: &str) -> Result<(), String> {
    validate_index_name(index)?;
    let index_url = resource_url(base_url, &[index])?;
    let exists = client
        .head(index_url.clone())
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if exists.status().is_success() {
        return verify_concrete_index(client, base_url, index).await;
    }
    if exists.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(format!(
            "could not check OpenSearch write index: {}",
            exists.status()
        ));
    }
    let body = serde_json::json!({ "settings": { "number_of_shards": 3, "number_of_replicas": 1, "analysis": { "analyzer": { "bookway_cjk": { "type": "cjk" } } } }, "mappings": { "dynamic": true, "properties": { "id": { "type": "text", "fields": { "keyword": { "type": "keyword" } } }, "title": { "type": "text", "analyzer": "bookway_cjk" }, "summary": { "type": "text", "analyzer": "bookway_cjk" }, "body": { "type": "text", "analyzer": "bookway_cjk" }, "status": { "type": "keyword" }, "author_id": { "type": "keyword" }, "content_type": { "type": "keyword" }, "domain": { "type": "keyword" }, "topics": { "type": "keyword" }, "tags": { "type": "keyword" } } } });
    let created = client
        .put(index_url)
        .json(&body)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !created.status().is_success()
        && !matches!(
            created.status(),
            reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::CONFLICT
        )
    {
        return Err(format!(
            "could not create OpenSearch index: {}",
            created.status()
        ));
    }
    // A concurrent creator may have won the race. Verify the resolved name
    // before accepting it so an alias can never become a document write path.
    verify_concrete_index(client, base_url, index).await
}

async fn verify_concrete_index(
    client: &reqwest::Client,
    base_url: &str,
    index: &str,
) -> Result<(), String> {
    let response = client
        .get(resource_url(base_url, &["_resolve", "index", index])?)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "could not resolve OpenSearch write index: {}",
            response.status()
        ));
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| error.to_string())?;
    if resolved_as_concrete_index(&payload, index) {
        Ok(())
    } else {
        Err(format!(
            "OPENSEARCH_WRITE_INDEX must name one concrete index, not an alias or data stream: {index}"
        ))
    }
}

fn validate_index_name(value: &str) -> Result<(), String> {
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
        return Err(format!("invalid OpenSearch write index name: {value}"));
    }
    Ok(())
}

fn resolved_as_concrete_index(payload: &Value, index: &str) -> bool {
    let has_exact_index = payload
        .get("indices")
        .and_then(Value::as_array)
        .is_some_and(|indices| {
            indices
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(index))
        });
    let has_same_named_alias = payload
        .get("aliases")
        .and_then(Value::as_array)
        .is_some_and(|aliases| {
            aliases
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(index))
        });
    let has_same_named_data_stream = payload
        .get("data_streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(index))
        });
    has_exact_index && !has_same_named_alias && !has_same_named_data_stream
}

fn resource_url(base_url: &str, path: &[&str]) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base_url).map_err(|error| error.to_string())?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| "OPENSEARCH_URL cannot be used as a base URL".to_string())?;
    segments.pop_if_empty();
    for segment in path {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

fn versioned_document_url(
    base_url: &str,
    index: &str,
    content_id: &str,
    version: i64,
) -> Result<reqwest::Url, String> {
    let mut url = resource_url(base_url, &[index, "_doc", content_id])?;
    url.query_pairs_mut()
        .append_pair("version", &version.to_string())
        .append_pair("version_type", "external_gte");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        IndexOperation, index_operation, resolved_as_concrete_index, resource_url,
        validate_index_name, versioned_document_url, write_indices,
    };

    #[test]
    fn only_published_content_is_searchable() {
        let operation = index_operation(json!({ "id": "post-1" }), "restricted");
        assert!(matches!(operation, IndexOperation::Delete));
    }

    #[test]
    fn published_content_projects_post_summary_to_search_fields() {
        let operation = index_operation(
            json!({
                "id": "post-1",
                "post": {
                    "title": "A useful route",
                    "summary": "Start with a small step",
                    "author_name": "author",
                    "tags": ["learning"]
                }
            }),
            "published",
        );
        let IndexOperation::Upsert(document) = operation else {
            panic!("published content should be indexed");
        };
        assert_eq!(document["title"], "A useful route");
        assert_eq!(document["summary"], "Start with a small step");
        assert_eq!(document["tags"], json!(["learning"]));
    }

    #[test]
    fn document_paths_escape_opaque_content_ids() {
        let url = resource_url("https://search.example/api", &["content", "_doc", "a/b"])
            .expect("valid URL");
        assert_eq!(
            url.as_str(),
            "https://search.example/api/content/_doc/a%2Fb"
        );
    }

    #[test]
    fn document_writes_use_monotonic_external_versions() {
        let url = versioned_document_url(
            "https://search.example/api",
            "bookway-content-v2",
            "post-1",
            42,
        )
        .expect("valid URL");
        assert_eq!(
            url.as_str(),
            "https://search.example/api/bookway-content-v2/_doc/post-1?version=42&version_type=external_gte"
        );
    }

    #[test]
    fn shadow_write_target_is_distinct_from_primary() {
        assert_eq!(
            write_indices(
                "bookway-content-v1".to_string(),
                Some("bookway-content-v2".to_string())
            )
            .expect("distinct targets"),
            ["bookway-content-v1", "bookway-content-v2"]
        );
        assert!(
            write_indices(
                "bookway-content-v1".to_string(),
                Some("bookway-content-v1".to_string())
            )
            .is_err()
        );
    }

    #[test]
    fn write_index_names_are_strict_and_never_system_indexes() {
        assert!(validate_index_name("bookway-content-v2").is_ok());
        for invalid in ["Bookway-v2", ".system", "bookway/*", "bookway v2", ".."] {
            assert!(
                validate_index_name(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    #[test]
    fn resolved_name_must_be_an_exact_concrete_index() {
        assert!(resolved_as_concrete_index(
            &json!({ "indices": [{ "name": "bookway-content-v2" }], "aliases": [], "data_streams": [] }),
            "bookway-content-v2"
        ));
        assert!(!resolved_as_concrete_index(
            &json!({ "indices": [], "aliases": [{ "name": "bookway-content", "indices": ["bookway-content-v2"] }], "data_streams": [] }),
            "bookway-content"
        ));
    }
}
