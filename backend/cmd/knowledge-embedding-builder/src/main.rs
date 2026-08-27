use std::{env, sync::Arc, time::Duration};

use bookway_knowledge_catalog_api::pb::{
    self,
    knowledge_catalog_client::KnowledgeCatalogClient,
};
use serde_json::Value;

const BATCH_SIZE: i64 = 20;
const MAX_ATTEMPTS: i32 = 10;
const LEASE_SECONDS: f64 = 600.0;
/// Dimensions are bounded by the 0072 migration CHECK.
const EMBEDDING_DIM_RANGE: std::ops::RangeInclusive<usize> = 8..=4096;
const HTTP_TIMEOUT_SECS: u64 = 30;

struct PendingEmbedding {
    attachment_id: String,
    route_id: String,
    action_node_id: String,
    created_by: String,
    document_text: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("knowledge-embedding-builder");
    let endpoint = required_env("RAG_EMBEDDING_ENDPOINT")?;
    let api_key = env::var("RAG_EMBEDDING_API_KEY")
        .ok()
        .filter(|key| !key.is_empty());
    let model = required_env("RAG_EMBEDDING_MODEL")?;
    let catalog_url = env::var("KNOWLEDGE_CATALOG_GRPC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8105".to_string());
    let client = KnowledgeCatalogClient::new(bookway_runtime::grpc_channel(&catalog_url).await?);
    let http = bookway_runtime::http_client();
    let pool = Arc::new(bookway_data::postgres_pool().await?);

    tracing::info!(%model, %catalog_url, "embedding builder started");
    loop {
        let jobs = match claim_jobs(&pool).await {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::error!(%error, "could not claim embedding jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if jobs.is_empty() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        for job in jobs {
            match embed_document(&http, &endpoint, api_key.as_deref(), &model, &job.document_text)
                .await
            {
                Ok(embedding) => {
                    if let Err(error) =
                        complete_embedding(&client, &pool, &job, &model, embedding).await
                    {
                        fail_job(&pool, &job, &error).await;
                    }
                }
                Err(error) => fail_job(&pool, &job, &error).await,
            }
        }
    }
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required for the embedding builder").into())
}

/// Absence of an embeddings row is the single source of truth for "pending":
/// the attempt counter, next-attempt timestamp and lease only pace retries and
/// never encode a state of their own. FOR UPDATE SKIP LOCKED keeps concurrent
/// builders apart; the lease window covers work between claim and completion.
async fn claim_jobs(pool: &sqlx::PgPool) -> Result<Vec<PendingEmbedding>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
        ),
    >(
        r#"
        WITH claimed AS (
            SELECT attachment.id
            FROM route_node_resource_attachments AS attachment
            WHERE attachment.rag_enabled
              AND attachment.archived_at IS NULL
              AND attachment.embedding_attempts < $1
              AND attachment.embedding_next_attempt_at <= now()
              AND (attachment.embedding_lease_until IS NULL OR attachment.embedding_lease_until <= now())
              AND NOT EXISTS (
                  SELECT 1 FROM route_node_resource_embeddings AS stored
                  WHERE stored.attachment_id = attachment.id
              )
            ORDER BY attachment.embedding_next_attempt_at
            LIMIT $2
            FOR UPDATE OF attachment SKIP LOCKED
        )
        UPDATE route_node_resource_attachments AS attachment
        SET embedding_attempts = attachment.embedding_attempts + 1,
            embedding_lease_until = now() + make_interval(secs => $3),
            updated_at = now()
        FROM claimed, public_resources AS resource
        WHERE attachment.id = claimed.id
          AND resource.id = attachment.resource_id
        RETURNING attachment.id, attachment.route_id, attachment.action_node_id,
                  attachment.created_by,
                  COALESCE(NULLIF(attachment.title_override, ''), resource.title),
                  NULLIF(attachment.note, ''),
                  resource.summary
        "#,
    )
    .bind(MAX_ATTEMPTS)
    .bind(BATCH_SIZE)
    .bind(LEASE_SECONDS)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(rows
        .into_iter()
        .map(|(id, route_id, action_node_id, created_by, title, note, summary)| {
            // Mirror knowledge-catalog's rag_excerpt choice so retrieval never
            // ranks against text the vector was not built from. Vectors are
            // derived from public metadata plus the creator's note only.
            let excerpt: String = match note.as_deref().map(str::trim) {
                Some(note) if !note.is_empty() => note.chars().take(600).collect(),
                _ => summary.chars().take(600).collect(),
            };
            PendingEmbedding {
                attachment_id: id,
                route_id,
                action_node_id,
                created_by,
                document_text: format!("{title}\n{excerpt}"),
            }
        })
        .collect())
}

// Locally repeats the OpenAI-compatible call the catalog service itself uses;
// the job must stay deployable without linking service internals.
async fn embed_document(
    http: &reqwest::Client,
    endpoint: &str,
    api_key: Option<&str>,
    model: &str,
    document_text: &str,
) -> Result<Vec<f32>, String> {
    let mut request = http
        .post(format!("{}/embeddings", endpoint.trim_end_matches('/')))
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .json(&serde_json::json!({
            "model": model,
            "input": [document_text],
        }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("embedding provider returned {}", response.status()));
    }
    let payload: Value = response.json().await.map_err(|e| e.to_string())?;
    let embedding = payload
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|first| first.get("embedding"))
        .cloned()
        .ok_or_else(|| "provider response has no embedding".to_string())?;
    let embedding = embedding
        .as_array()
        .ok_or_else(|| "provider embedding is not a list".to_string())?
        .iter()
        .map(|value| {
            value
                .as_f64()
                .map(|number| number as f32)
                .ok_or_else(|| "provider embedding has non-numeric values".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !EMBEDDING_DIM_RANGE.contains(&embedding.len()) {
        return Err(format!(
            "provider returned {} dimensions; expected {EMBEDDING_DIM_RANGE:?}",
            embedding.len()
        ));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err("provider embedding contains non-finite values".to_string());
    }
    Ok(embedding)
}

async fn complete_embedding(
    client: &KnowledgeCatalogClient<tonic::transport::Channel>,
    pool: &sqlx::PgPool,
    job: &PendingEmbedding,
    model: &str,
    embedding: Vec<f32>,
) -> Result<(), String> {
    let mut client = client.clone();
    client
        .upsert_rag_embedding(
            bookway_runtime::grpc_service_request(pb::UpsertRagEmbeddingRequest {
                route_id: job.route_id.clone(),
                action_node_id: job.action_node_id.clone(),
                attachment_id: job.attachment_id.clone(),
                embedding_model: model.to_string(),
                embedding,
                operator_id: job.created_by.clone(),
            })
            .map_err(|error| error.to_string())?,
        )
        .await
        .map_err(|status| status.message().to_string())?;
    release_lease(pool, job).await;
    Ok(())
}

/// Failure pacing: exponential backoff capped at five minutes. A failed row
/// becomes visible again once its backoff expires and stays eligible until the
/// scan's MAX_ATTEMPTS predicate dead-letters it; last_error keeps the reason
/// inspectable for operators.
async fn fail_job(pool: &sqlx::PgPool, job: &PendingEmbedding, error: &str) {
    match sqlx::query(
        r#"
        UPDATE route_node_resource_attachments
        SET embedding_next_attempt_at = now() + make_interval(
                secs => LEAST(300, CAST(power(2, embedding_attempts) AS INTEGER))
            ),
            embedding_last_error = left($2, 2000),
            embedding_lease_until = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(&job.attachment_id)
    .bind(error)
    .execute(pool)
    .await
    {
        Ok(_) => tracing::warn!(
            attachment_id = %job.attachment_id,
            %error,
            "rag embedding failed; scheduled for retry"
        ),
        Err(update_error) => tracing::warn!(
            %update_error,
            attachment_id = %job.attachment_id,
            "could not record embedding failure"
        ),
    }
}

async fn release_lease(pool: &sqlx::PgPool, job: &PendingEmbedding) {
    if let Err(error) = sqlx::query(
        r#"
        UPDATE route_node_resource_attachments
        SET embedding_last_error = '',
            embedding_lease_until = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(&job.attachment_id)
    .execute(pool)
    .await
    {
        tracing::debug!(
            %error,
            attachment_id = %job.attachment_id,
            "could not clear embedding lease"
        );
    }
}
