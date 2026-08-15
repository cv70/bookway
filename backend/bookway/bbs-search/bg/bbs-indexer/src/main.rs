use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-indexer");
    let pool = bookway_data::postgres_pool().await?;
    let client = bookway_runtime::http_client();
    let url = env::var("OPENSEARCH_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9200".to_string())
        .trim_end_matches('/')
        .to_string();
    let index = env::var("OPENSEARCH_INDEX").unwrap_or_else(|_| "bookway-content-v1".to_string());
    ensure_index(&client, &url, &index).await?;
    let mut cursor_time = time::OffsetDateTime::UNIX_EPOCH;
    let mut cursor_id = String::new();
    loop {
        let rows = sqlx::query_as::<_, (String, serde_json::Value, String, time::OffsetDateTime)>("SELECT id,payload,status,updated_at FROM content_items WHERE (updated_at,id) > ($1,$2) ORDER BY updated_at,id LIMIT 500")
            .bind(cursor_time).bind(&cursor_id).fetch_all(&pool).await?;
        if rows.is_empty() {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let mut retry_required = false;
        for (id, mut document, status, updated_at) in rows {
            let synchronized = if status != "published" {
                match client
                    .delete(format!("{}/{}/_doc/{}", url, index, id))
                    .send()
                    .await
                {
                    Ok(response)
                        if response.status().is_success()
                            || response.status() == reqwest::StatusCode::NOT_FOUND =>
                    {
                        true
                    }
                    Ok(response) => {
                        tracing::warn!(id, status = %response.status(), "opensearch deletion failed; cursor retained");
                        false
                    }
                    Err(error) => {
                        tracing::warn!(id, %error, "opensearch deletion failed; cursor retained");
                        false
                    }
                }
            } else {
                if let Some(post) = document.get("post").cloned()
                    && let Some(object) = document.as_object_mut()
                    && let Some(post_object) = post.as_object()
                {
                    for field in ["title", "summary", "author_name", "tags"] {
                        if let Some(value) = post_object.get(field) {
                            object.insert(field.to_string(), value.clone());
                        }
                    }
                }
                match client
                    .put(format!("{}/{}/_doc/{}", url, index, id))
                    .json(&document)
                    .send()
                    .await
                {
                    Ok(response) if response.status().is_success() => true,
                    Ok(response) => {
                        tracing::warn!(id, status = %response.status(), "opensearch indexing failed; cursor retained");
                        false
                    }
                    Err(error) => {
                        tracing::warn!(id, %error, "opensearch indexing failed; cursor retained");
                        false
                    }
                }
            };
            if !synchronized {
                retry_required = true;
                break;
            }
            cursor_time = updated_at;
            cursor_id = id;
        }
        if retry_required {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

async fn ensure_index(
    client: &reqwest::Client,
    url: &str,
    index: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let exists = client.head(format!("{url}/{index}")).send().await?;
    if exists.status().is_success() {
        return Ok(());
    }
    let body = serde_json::json!({ "settings": { "number_of_shards": 3, "number_of_replicas": 1, "analysis": { "analyzer": { "bookway_cjk": { "type": "cjk" } } } }, "mappings": { "dynamic": true, "properties": { "id": { "type": "text", "fields": { "keyword": { "type": "keyword" } } }, "title": { "type": "text", "analyzer": "bookway_cjk" }, "summary": { "type": "text", "analyzer": "bookway_cjk" }, "body": { "type": "text", "analyzer": "bookway_cjk" }, "status": { "type": "keyword" }, "author_id": { "type": "keyword" }, "content_type": { "type": "keyword" }, "domain": { "type": "keyword" }, "topics": { "type": "keyword" }, "tags": { "type": "keyword" } } } });
    let created = client
        .put(format!("{url}/{index}"))
        .json(&body)
        .send()
        .await?;
    if !created.status().is_success() && created.status().as_u16() != 400 {
        created.error_for_status()?;
    }
    Ok(())
}
