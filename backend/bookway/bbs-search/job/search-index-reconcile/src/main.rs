use std::{
    collections::{HashMap, HashSet},
    env,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

const DEFAULT_BATCH_SIZE: i64 = 500;
const MAX_BATCH_SIZE: i64 = 2_000;
const DEFAULT_SAMPLE_LIMIT: usize = 0;
const MAX_SAMPLE_LIMIT: usize = 100;
const DEFAULT_LEASE_SECONDS: i32 = 600;
const MAX_LEASE_SECONDS: i32 = 24 * 60 * 60;

#[derive(Debug, Error)]
enum ReconcileError {
    #[error("{key} is required")]
    MissingEnvironment { key: &'static str },
    #[error("invalid {key}: {value}")]
    InvalidEnvironment { key: &'static str, value: String },
    #[error("OpenSearch request failed: {0}")]
    Request(String),
    #[error("OpenSearch reconciliation index is missing: {0}")]
    TargetMissing(String),
    #[error("OpenSearch reconciliation target must resolve to one concrete index: {0}")]
    TargetNotConcrete(String),
    #[error("OpenSearch mget response is invalid: {0}")]
    MgetResponse(String),
    #[error("database operation failed: {0}")]
    Database(String),
    #[error("content has an invalid version")]
    InvalidContentVersion,
    #[error("OpenSearch returned an invalid document count")]
    InvalidTargetCount,
    #[error("reconciliation run {0} was not found")]
    RunNotFound(Uuid),
    #[error("reconciliation run {0} has a different target index")]
    RunTargetMismatch(Uuid),
    #[error("reconciliation run {run_id} cannot be resumed from status {status}")]
    RunNotResumable { run_id: Uuid, status: String },
    #[error("reconciliation run {0} is leased by another worker")]
    RunBusy(Uuid),
    #[error("reconciliation run lease was replaced")]
    RunLeaseLost,
}

#[derive(Debug)]
struct Config {
    base_url: String,
    target_index: String,
    batch_size: i64,
    start_after_id: String,
    sample_limit: usize,
    run_id: Option<Uuid>,
    lease_seconds: i32,
}

impl Config {
    fn from_env() -> Result<Self, ReconcileError> {
        let base_url = required_env("OPENSEARCH_URL")?;
        let target_index = required_env("OPENSEARCH_RECONCILE_INDEX")?;
        validate_resource_name("OPENSEARCH_RECONCILE_INDEX", &target_index)?;
        let start_after_id = optional_env("SEARCH_INDEX_RECONCILE_AFTER_ID").unwrap_or_default();
        let run_id = optional_env("SEARCH_INDEX_RECONCILE_RUN_ID")
            .map(|value| {
                Uuid::parse_str(&value).map_err(|_| ReconcileError::InvalidEnvironment {
                    key: "SEARCH_INDEX_RECONCILE_RUN_ID",
                    value,
                })
            })
            .transpose()?;
        if run_id.is_some() && !start_after_id.is_empty() {
            return Err(ReconcileError::InvalidEnvironment {
                key: "SEARCH_INDEX_RECONCILE_AFTER_ID",
                value: "cannot be set while resuming SEARCH_INDEX_RECONCILE_RUN_ID".to_string(),
            });
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            target_index,
            batch_size: env_number("SEARCH_INDEX_RECONCILE_BATCH_SIZE", DEFAULT_BATCH_SIZE)?
                .clamp(1, MAX_BATCH_SIZE),
            start_after_id,
            sample_limit: env_number("SEARCH_INDEX_RECONCILE_SAMPLE_LIMIT", DEFAULT_SAMPLE_LIMIT)?
                .clamp(0, MAX_SAMPLE_LIMIT),
            run_id,
            lease_seconds: env_number(
                "SEARCH_INDEX_RECONCILE_LEASE_SECONDS",
                DEFAULT_LEASE_SECONDS,
            )?
            .clamp(1, MAX_LEASE_SECONDS),
        })
    }
}

fn required_env(key: &'static str) -> Result<String, ReconcileError> {
    optional_env(key).ok_or(ReconcileError::MissingEnvironment { key })
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_number<T>(key: &'static str, default: T) -> Result<T, ReconcileError>
where
    T: std::str::FromStr,
{
    match optional_env(key) {
        Some(value) => value
            .parse()
            .map_err(|_| ReconcileError::InvalidEnvironment { key, value }),
        None => Ok(default),
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ContentRow {
    id: String,
    version: i64,
    status: String,
    deleted: bool,
}

impl ContentRow {
    fn should_be_indexed(&self) -> Result<bool, ReconcileError> {
        if self.version <= 0 {
            return Err(ReconcileError::InvalidContentVersion);
        }
        Ok(self.status == "published" && !self.deleted)
    }
}

#[derive(Debug, Deserialize)]
struct MultiGetResponse {
    docs: Vec<MultiGetDocument>,
}

#[derive(Debug, Deserialize)]
struct MultiGetDocument {
    #[serde(rename = "_id")]
    id: String,
    found: bool,
    #[serde(rename = "_source")]
    source: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ReconciliationStats {
    scanned: i64,
    expected_public: i64,
    expected_absent: i64,
    missing: i64,
    stale: i64,
    unexpected_present: i64,
}

#[derive(Default, Serialize)]
struct ReconciliationSamples {
    missing: Vec<String>,
    stale: Vec<String>,
    unexpected_present: Vec<String>,
}

#[derive(Serialize)]
struct ReconciliationResult {
    status: &'static str,
    run_id: String,
    target_index: String,
    full_scan: bool,
    scanned: i64,
    expected_public: i64,
    expected_absent: i64,
    missing: i64,
    stale: i64,
    unexpected_present: i64,
    source_count: i64,
    target_count: i64,
    outbox_pending: i64,
    outbox_processing: i64,
    outbox_dead: i64,
    healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    samples: Option<ReconciliationSamples>,
}

#[derive(sqlx::FromRow)]
struct OutboxLiveness {
    pending: i64,
    processing: i64,
    dead: i64,
}

impl OutboxLiveness {
    fn is_drained(&self) -> bool {
        self.pending == 0 && self.processing == 0 && self.dead == 0
    }
}

#[derive(Debug, sqlx::FromRow)]
struct StoredRun {
    id: Uuid,
    target_index: String,
    status: String,
    full_scan: bool,
    batch_size: i32,
    next_after_id: String,
    scanned: i64,
    expected_public: i64,
    expected_absent: i64,
    missing: i64,
    stale: i64,
    unexpected_present: i64,
    lease_active: bool,
}

#[derive(Debug)]
struct RunState {
    id: Uuid,
    lease_id: Uuid,
    target_index: String,
    full_scan: bool,
    batch_size: i64,
    next_after_id: String,
    stats: ReconciliationStats,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-index-reconcile");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let client = bookway_runtime::http_client();
    validate_target(&client, &config).await?;
    let mut run = acquire_run(&pool, &config).await?;
    let run_id = run.id;
    tracing::info!(
        run_id = %run_id,
        target_index = %run.target_index,
        resumed = config.run_id.is_some(),
        "search index reconciliation run started"
    );
    match reconcile_run(&pool, &client, &config, &mut run).await {
        Ok(result) => {
            tracing::info!(
                run_id = %result.run_id,
                target_index = %result.target_index,
                healthy = result.healthy,
                full_scan = result.full_scan,
                source_count = result.source_count,
                target_count = result.target_count,
                outbox_pending = result.outbox_pending,
                outbox_processing = result.outbox_processing,
                outbox_dead = result.outbox_dead,
                "search index reconciliation completed"
            );
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
        Err(error) => {
            if let Err(record_error) = mark_failed(&pool, run_id, run.lease_id, &error).await {
                tracing::error!(run_id = %run_id, %record_error, "could not record reconciliation failure");
            }
            Err(Box::new(error) as Box<dyn std::error::Error>)
        }
    }
}

async fn reconcile_run(
    pool: &sqlx::PgPool,
    client: &reqwest::Client,
    config: &Config,
    run: &mut RunState,
) -> Result<ReconciliationResult, ReconcileError> {
    refresh_target(client, config).await?;
    let mut samples = ReconciliationSamples::default();
    let mut batches = 0_i64;
    loop {
        let rows = load_batch(pool, &run.next_after_id, run.batch_size).await?;
        if rows.is_empty() {
            break;
        }
        let next_after_id = rows.last().map(|row| row.id.clone()).unwrap_or_default();
        let documents = multi_get(client, config, &rows).await?;
        reconcile_batch(
            &rows,
            documents,
            &mut run.stats,
            &mut samples,
            config.sample_limit,
        )?;
        run.next_after_id = next_after_id;
        store_checkpoint(pool, run).await?;
        batches = batches.saturating_add(1);
        tracing::info!(
            run_id = %run.id,
            target_index = %run.target_index,
            batch = batches,
            scanned = run.stats.scanned,
            missing = run.stats.missing,
            stale = run.stats.stale,
            unexpected_present = run.stats.unexpected_present,
            "search index reconciliation batch completed"
        );
    }

    refresh_target(client, config).await?;
    let source_count = source_count(pool).await?;
    let target_count = target_count(client, config).await?;
    let outbox = outbox_liveness(pool).await?;
    let healthy = run.full_scan
        && run.stats.missing == 0
        && run.stats.stale == 0
        && run.stats.unexpected_present == 0
        && source_count == target_count
        && outbox.is_drained();
    complete_run(pool, run, source_count, target_count, &outbox, healthy).await?;
    Ok(ReconciliationResult {
        status: "completed",
        run_id: run.id.to_string(),
        target_index: run.target_index.clone(),
        full_scan: run.full_scan,
        scanned: run.stats.scanned,
        expected_public: run.stats.expected_public,
        expected_absent: run.stats.expected_absent,
        missing: run.stats.missing,
        stale: run.stats.stale,
        unexpected_present: run.stats.unexpected_present,
        source_count,
        target_count,
        outbox_pending: outbox.pending,
        outbox_processing: outbox.processing,
        outbox_dead: outbox.dead,
        healthy,
        samples: (config.sample_limit > 0).then_some(samples),
    })
}

async fn acquire_run(pool: &sqlx::PgPool, config: &Config) -> Result<RunState, ReconcileError> {
    match config.run_id {
        Some(run_id) => resume_run(pool, config, run_id).await,
        None => create_run(pool, config).await,
    }
}

async fn create_run(pool: &sqlx::PgPool, config: &Config) -> Result<RunState, ReconcileError> {
    let id = Uuid::now_v7();
    let lease_id = Uuid::now_v7();
    let full_scan = config.start_after_id.is_empty();
    sqlx::query(
        "INSERT INTO content_index_reconciliation_runs (id,target_index,status,full_scan,batch_size,lease_seconds,next_after_id,lease_id,locked_at,updated_at) VALUES ($1,$2,'running',$3,$4,$5,$6,$7,now(),now())",
    )
    .bind(id)
    .bind(&config.target_index)
    .bind(full_scan)
    .bind(config.batch_size as i32)
    .bind(config.lease_seconds)
    .bind(&config.start_after_id)
    .bind(lease_id)
    .execute(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))?;
    Ok(RunState {
        id,
        lease_id,
        target_index: config.target_index.clone(),
        full_scan,
        batch_size: config.batch_size,
        next_after_id: config.start_after_id.clone(),
        stats: ReconciliationStats::default(),
    })
}

async fn resume_run(
    pool: &sqlx::PgPool,
    config: &Config,
    run_id: Uuid,
) -> Result<RunState, ReconcileError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| ReconcileError::Database(error.to_string()))?;
    let stored = sqlx::query_as::<_, StoredRun>(
        "SELECT id,target_index,status,full_scan,batch_size,next_after_id,scanned,expected_public,expected_absent,missing,stale,unexpected_present,COALESCE(locked_at > now() - make_interval(secs => lease_seconds), false) AS lease_active FROM content_index_reconciliation_runs WHERE id = $1 FOR UPDATE",
    )
    .bind(run_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))?
    .ok_or(ReconcileError::RunNotFound(run_id))?;
    if stored.target_index != config.target_index {
        return Err(ReconcileError::RunTargetMismatch(run_id));
    }
    if !matches!(stored.status.as_str(), "running" | "failed") {
        return Err(ReconcileError::RunNotResumable {
            run_id,
            status: stored.status,
        });
    }
    if stored.lease_active {
        return Err(ReconcileError::RunBusy(run_id));
    }
    let lease_id = Uuid::now_v7();
    sqlx::query(
        "UPDATE content_index_reconciliation_runs SET status = 'running', lease_id = $2, locked_at = now(), last_error = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(lease_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))?;
    transaction
        .commit()
        .await
        .map_err(|error| ReconcileError::Database(error.to_string()))?;
    Ok(RunState {
        id: stored.id,
        lease_id,
        target_index: stored.target_index,
        full_scan: stored.full_scan,
        batch_size: i64::from(stored.batch_size),
        next_after_id: stored.next_after_id,
        stats: ReconciliationStats {
            scanned: stored.scanned,
            expected_public: stored.expected_public,
            expected_absent: stored.expected_absent,
            missing: stored.missing,
            stale: stored.stale,
            unexpected_present: stored.unexpected_present,
        },
    })
}

async fn store_checkpoint(pool: &sqlx::PgPool, run: &RunState) -> Result<(), ReconcileError> {
    let result = sqlx::query(
        "UPDATE content_index_reconciliation_runs SET next_after_id = $3, scanned = $4, expected_public = $5, expected_absent = $6, missing = $7, stale = $8, unexpected_present = $9, locked_at = now(), updated_at = now() WHERE id = $1 AND lease_id = $2 AND status = 'running'",
    )
    .bind(run.id)
    .bind(run.lease_id)
    .bind(&run.next_after_id)
    .bind(run.stats.scanned)
    .bind(run.stats.expected_public)
    .bind(run.stats.expected_absent)
    .bind(run.stats.missing)
    .bind(run.stats.stale)
    .bind(run.stats.unexpected_present)
    .execute(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ReconcileError::RunLeaseLost)
    }
}

async fn complete_run(
    pool: &sqlx::PgPool,
    run: &RunState,
    source_count: i64,
    target_count: i64,
    outbox: &OutboxLiveness,
    healthy: bool,
) -> Result<(), ReconcileError> {
    let result = sqlx::query(
        "UPDATE content_index_reconciliation_runs SET status = 'completed', source_count = $3, target_count = $4, outbox_pending = $5, outbox_processing = $6, outbox_dead = $7, healthy = $8, lease_id = NULL, locked_at = NULL, last_error = NULL, updated_at = now(), completed_at = now() WHERE id = $1 AND lease_id = $2 AND status = 'running'",
    )
    .bind(run.id)
    .bind(run.lease_id)
    .bind(source_count)
    .bind(target_count)
    .bind(outbox.pending)
    .bind(outbox.processing)
    .bind(outbox.dead)
    .bind(healthy)
    .execute(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ReconcileError::RunLeaseLost)
    }
}

async fn mark_failed(
    pool: &sqlx::PgPool,
    run_id: Uuid,
    lease_id: Uuid,
    error: &ReconcileError,
) -> Result<(), ReconcileError> {
    let result = sqlx::query(
        "UPDATE content_index_reconciliation_runs SET status = 'failed', lease_id = NULL, locked_at = NULL, last_error = left($3, 2000), updated_at = now() WHERE id = $1 AND lease_id = $2 AND status = 'running'",
    )
    .bind(run_id)
    .bind(lease_id)
    .bind(error.to_string())
    .execute(pool)
    .await
    .map_err(|database_error| ReconcileError::Database(database_error.to_string()))?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ReconcileError::RunLeaseLost)
    }
}

async fn load_batch(
    pool: &sqlx::PgPool,
    after_id: &str,
    batch_size: i64,
) -> Result<Vec<ContentRow>, ReconcileError> {
    sqlx::query_as::<_, ContentRow>(
        "SELECT id, version, status, deleted_at IS NOT NULL AS deleted FROM content_items WHERE id > $1 ORDER BY id LIMIT $2",
    )
    .bind(after_id)
    .bind(batch_size)
    .fetch_all(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))
}

async fn multi_get(
    client: &reqwest::Client,
    config: &Config,
    rows: &[ContentRow],
) -> Result<Vec<MultiGetDocument>, ReconcileError> {
    let ids = rows.iter().map(|row| &row.id).collect::<Vec<_>>();
    let response = client
        .post(resource_url(
            &config.base_url,
            &[&config.target_index, "_mget"],
        )?)
        .json(&json!({ "ids": ids }))
        .send()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ReconcileError::Request(format!(
            "mget reconciliation returned {}",
            response.status()
        )));
    }
    response
        .json::<MultiGetResponse>()
        .await
        .map(|response| response.docs)
        .map_err(|error| ReconcileError::MgetResponse(error.to_string()))
}

fn reconcile_batch(
    rows: &[ContentRow],
    documents: Vec<MultiGetDocument>,
    stats: &mut ReconciliationStats,
    samples: &mut ReconciliationSamples,
    sample_limit: usize,
) -> Result<(), ReconcileError> {
    if documents.len() != rows.len() {
        return Err(ReconcileError::MgetResponse(format!(
            "expected {} documents but received {}",
            rows.len(),
            documents.len()
        )));
    }
    let requested_ids = rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<HashSet<_>>();
    let mut response_by_id = HashMap::with_capacity(documents.len());
    for document in documents {
        if !requested_ids.contains(document.id.as_str()) {
            return Err(ReconcileError::MgetResponse(
                "received an unrequested document".to_string(),
            ));
        }
        if response_by_id
            .insert(document.id.clone(), document)
            .is_some()
        {
            return Err(ReconcileError::MgetResponse(
                "received a duplicate document ID".to_string(),
            ));
        }
    }
    if response_by_id.len() != rows.len() {
        return Err(ReconcileError::MgetResponse(
            "response does not cover every requested document".to_string(),
        ));
    }

    for row in rows {
        let document = response_by_id.get(&row.id).ok_or_else(|| {
            ReconcileError::MgetResponse(
                "response does not cover every requested document".to_string(),
            )
        })?;
        stats.scanned = stats.scanned.saturating_add(1);
        if row.should_be_indexed()? {
            stats.expected_public = stats.expected_public.saturating_add(1);
            if !document.found {
                stats.missing = stats.missing.saturating_add(1);
                record_sample(&mut samples.missing, &row.id, sample_limit);
                continue;
            }
            let source = document.source.as_ref().ok_or_else(|| {
                ReconcileError::MgetResponse("found document has no _source".to_string())
            })?;
            if source.get("version").and_then(Value::as_i64) != Some(row.version) {
                stats.stale = stats.stale.saturating_add(1);
                record_sample(&mut samples.stale, &row.id, sample_limit);
            }
        } else {
            stats.expected_absent = stats.expected_absent.saturating_add(1);
            if document.found {
                stats.unexpected_present = stats.unexpected_present.saturating_add(1);
                record_sample(&mut samples.unexpected_present, &row.id, sample_limit);
            }
        }
    }
    Ok(())
}

fn record_sample(samples: &mut Vec<String>, content_id: &str, sample_limit: usize) {
    if samples.len() < sample_limit {
        samples.push(content_id.to_string());
    }
}

async fn validate_target(client: &reqwest::Client, config: &Config) -> Result<(), ReconcileError> {
    let target_url = resource_url(&config.base_url, &[&config.target_index])?;
    let exists = client
        .head(target_url)
        .send()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if exists.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(ReconcileError::TargetMissing(config.target_index.clone()));
    }
    if !exists.status().is_success() {
        return Err(ReconcileError::Request(format!(
            "reconciliation target check returned {}",
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
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if !resolved.status().is_success() {
        return Err(ReconcileError::Request(format!(
            "reconciliation target resolution returned {}",
            resolved.status()
        )));
    }
    let payload = resolved
        .json::<Value>()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if resolves_to_concrete_index(&payload, &config.target_index) {
        Ok(())
    } else {
        Err(ReconcileError::TargetNotConcrete(
            config.target_index.clone(),
        ))
    }
}

async fn refresh_target(client: &reqwest::Client, config: &Config) -> Result<(), ReconcileError> {
    let response = client
        .post(resource_url(
            &config.base_url,
            &[&config.target_index, "_refresh"],
        )?)
        .send()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(ReconcileError::Request(format!(
            "reconciliation target refresh returned {}",
            response.status()
        )))
    }
}

async fn source_count(pool: &sqlx::PgPool) -> Result<i64, ReconcileError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM content_items WHERE status = 'published' AND deleted_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))
}

async fn outbox_liveness(pool: &sqlx::PgPool) -> Result<OutboxLiveness, ReconcileError> {
    sqlx::query_as::<_, OutboxLiveness>(
        "SELECT COUNT(*) FILTER (WHERE status = 'pending')::BIGINT AS pending, COUNT(*) FILTER (WHERE status = 'processing')::BIGINT AS processing, COUNT(*) FILTER (WHERE status = 'dead')::BIGINT AS dead FROM content_index_outbox",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| ReconcileError::Database(error.to_string()))
}

async fn target_count(client: &reqwest::Client, config: &Config) -> Result<i64, ReconcileError> {
    let response = client
        .get(resource_url(
            &config.base_url,
            &[&config.target_index, "_count"],
        )?)
        .send()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ReconcileError::Request(format!(
            "reconciliation target count returned {}",
            response.status()
        )));
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| ReconcileError::Request(error.to_string()))?;
    payload
        .get("count")
        .and_then(Value::as_i64)
        .filter(|count| *count >= 0)
        .ok_or(ReconcileError::InvalidTargetCount)
}

fn resolves_to_concrete_index(payload: &Value, target_index: &str) -> bool {
    let has_exact_index = payload
        .get("indices")
        .and_then(Value::as_array)
        .is_some_and(|indices| {
            indices.len() == 1
                && indices[0].get("name").and_then(Value::as_str) == Some(target_index)
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

fn validate_resource_name(key: &'static str, value: &str) -> Result<(), ReconcileError> {
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
        return Err(ReconcileError::InvalidEnvironment {
            key,
            value: value.to_string(),
        });
    }
    Ok(())
}

fn resource_url(base_url: &str, path: &[&str]) -> Result<reqwest::Url, ReconcileError> {
    let mut url =
        reqwest::Url::parse(base_url).map_err(|error| ReconcileError::InvalidEnvironment {
            key: "OPENSEARCH_URL",
            value: error.to_string(),
        })?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| ReconcileError::InvalidEnvironment {
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
    use serde_json::json;

    use super::{
        ContentRow, MultiGetDocument, MultiGetResponse, OutboxLiveness, ReconciliationResult,
        ReconciliationSamples, ReconciliationStats, reconcile_batch, resolves_to_concrete_index,
        resource_url, validate_resource_name,
    };

    fn row(id: &str, version: i64, status: &str, deleted: bool) -> ContentRow {
        ContentRow {
            id: id.to_string(),
            version,
            status: status.to_string(),
            deleted,
        }
    }

    fn documents(value: serde_json::Value) -> Vec<MultiGetDocument> {
        serde_json::from_value::<MultiGetResponse>(value)
            .expect("valid mget response")
            .docs
    }

    #[test]
    fn reconciliation_classifies_missing_stale_and_unexpected_documents() {
        let rows = vec![
            row("missing", 1, "published", false),
            row("stale", 2, "published", false),
            row("unexpected", 3, "reviewing", false),
            row("current", 4, "published", false),
        ];
        let documents = documents(json!({
            "docs": [
                { "_id": "missing", "found": false },
                { "_id": "stale", "found": true, "_source": { "version": 1 } },
                { "_id": "unexpected", "found": true, "_source": { "version": 3 } },
                { "_id": "current", "found": true, "_source": { "version": 4 } }
            ]
        }));
        let mut stats = ReconciliationStats::default();
        let mut samples = ReconciliationSamples::default();
        reconcile_batch(&rows, documents, &mut stats, &mut samples, 1).expect("valid batch");

        assert_eq!(stats.scanned, 4);
        assert_eq!(stats.expected_public, 3);
        assert_eq!(stats.expected_absent, 1);
        assert_eq!(stats.missing, 1);
        assert_eq!(stats.stale, 1);
        assert_eq!(stats.unexpected_present, 1);
        assert_eq!(samples.missing, ["missing"]);
        assert_eq!(samples.stale, ["stale"]);
        assert_eq!(samples.unexpected_present, ["unexpected"]);
    }

    #[test]
    fn reconciliation_rejects_unrequested_and_duplicate_mget_documents() {
        let rows = vec![row("post-1", 1, "published", false)];
        let mut stats = ReconciliationStats::default();
        let mut samples = ReconciliationSamples::default();
        let unrequested = documents(json!({
            "docs": [{ "_id": "post-2", "found": false }]
        }));
        assert!(reconcile_batch(&rows, unrequested, &mut stats, &mut samples, 0).is_err());

        let duplicate = documents(json!({
            "docs": [
                { "_id": "post-1", "found": false },
                { "_id": "post-1", "found": false }
            ]
        }));
        assert!(reconcile_batch(&rows, duplicate, &mut stats, &mut samples, 0).is_err());
    }

    #[test]
    fn concrete_index_and_url_validation_are_strict() {
        assert!(resolves_to_concrete_index(
            &json!({ "indices": [{ "name": "bookway-content-v2" }], "aliases": [], "data_streams": [] }),
            "bookway-content-v2"
        ));
        assert!(!resolves_to_concrete_index(
            &json!({ "indices": [{ "name": "bookway-content-v2" }], "aliases": [{ "name": "bookway-content-v2" }], "data_streams": [] }),
            "bookway-content-v2"
        ));
        assert!(!resolves_to_concrete_index(
            &json!({ "indices": [{ "name": "bookway-content-v2" }, { "name": "bookway-content-v3" }], "aliases": [], "data_streams": [] }),
            "bookway-content-v2"
        ));
        assert!(validate_resource_name("key", "bookway-content-v2").is_ok());
        assert!(validate_resource_name("key", ".system").is_err());
        let url = resource_url("https://search.example/api", &["index", "a/b"]).expect("valid URL");
        assert_eq!(url.as_str(), "https://search.example/api/index/a%2Fb");
    }

    #[test]
    fn aggregate_result_omits_internal_ids_without_explicit_diagnostic_options() {
        let result = ReconciliationResult {
            status: "completed",
            run_id: "0198e401-7dec-7000-8000-000000000001".to_string(),
            target_index: "bookway-content-v2".to_string(),
            full_scan: true,
            scanned: 1,
            expected_public: 1,
            expected_absent: 0,
            missing: 0,
            stale: 0,
            unexpected_present: 0,
            source_count: 1,
            target_count: 1,
            outbox_pending: 0,
            outbox_processing: 0,
            outbox_dead: 0,
            healthy: true,
            samples: None,
        };
        let output = serde_json::to_value(result).expect("serializes");
        assert!(output.get("samples").is_none());
        assert!(output.get("next_after_id").is_none());
    }

    #[test]
    fn non_delivered_outbox_jobs_block_the_publish_gate() {
        assert!(
            OutboxLiveness {
                pending: 0,
                processing: 0,
                dead: 0,
            }
            .is_drained()
        );
        assert!(
            !OutboxLiveness {
                pending: 1,
                processing: 0,
                dead: 0,
            }
            .is_drained()
        );
        assert!(
            !OutboxLiveness {
                pending: 0,
                processing: 1,
                dead: 1,
            }
            .is_drained()
        );
    }
}
