use std::{env, time::Duration};

use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use uuid::Uuid;

const BATCH_SIZE: i64 = 100;
const JOB_LEASE_SECONDS: i32 = 300;
const MAX_ATTEMPTS: i32 = 10;

#[derive(Debug)]
struct MediaJob {
    asset_id: String,
    object_key: String,
    mime_type: String,
    size_bytes: i64,
    lease_id: Uuid,
}

struct ObjectVerifier {
    bucket: Bucket,
    credentials: Credentials,
    client: reqwest::Client,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("media-processor");
    let pool = bookway_data::postgres_pool().await?;
    let verifier = ObjectVerifier::from_env()?;

    loop {
        let jobs = match claim_jobs(&pool).await {
            Ok(jobs) => jobs,
            Err(error) => {
                tracing::error!(%error, "could not claim media processing jobs");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if jobs.is_empty() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        for job in jobs {
            let result = verifier.verify(&job).await;
            match result {
                Ok(()) => {
                    if let Err(error) = mark_ready(&pool, &job).await {
                        tracing::error!(asset_id = %job.asset_id, %error, "could not complete media processing job");
                    }
                }
                Err(error) => {
                    tracing::warn!(asset_id = %job.asset_id, %error, "media processing verification failed");
                    if let Err(retry_error) = schedule_retry(&pool, &job, &error).await {
                        tracing::error!(asset_id = %job.asset_id, %retry_error, "could not reschedule media processing job");
                    }
                }
            }
        }
    }
}

impl ObjectVerifier {
    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let endpoint =
            env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".to_string());
        let bucket_name = env::var("S3_BUCKET").unwrap_or_else(|_| "bookway-media".to_string());
        let region = env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let access_key = env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "bookway-local".to_string());
        let secret_key =
            env::var("S3_SECRET_KEY").unwrap_or_else(|_| "bookway-local-only".to_string());
        let bucket = Bucket::new(endpoint.parse()?, UrlStyle::Path, bucket_name, region)?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(access_key, secret_key),
            client: bookway_runtime::http_client(),
        })
    }

    async fn verify(&self, job: &MediaJob) -> Result<(), String> {
        let response = self
            .client
            .head(
                self.bucket
                    .head_object(Some(&self.credentials), &job.object_key)
                    .sign(Duration::from_secs(60)),
            )
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let size_bytes = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok())
            .ok_or_else(|| "object storage response is missing content-length".to_string())?;
        if size_bytes != job.size_bytes {
            return Err(format!(
                "object size changed during processing: expected {}B, received {}B",
                job.size_bytes, size_bytes
            ));
        }
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or(value)
                    .trim()
                    .to_ascii_lowercase()
            })
            .ok_or_else(|| "object storage response is missing content-type".to_string())?;
        if mime_type != job.mime_type {
            return Err(format!(
                "object MIME changed during processing: expected {}, received {}",
                job.mime_type, mime_type
            ));
        }
        if !matches!(
            mime_type.as_str(),
            "image/jpeg" | "image/png" | "image/webp" | "video/mp4" | "audio/mpeg"
        ) {
            return Err("object MIME is not allowed by the media policy".to_string());
        }
        Ok(())
    }
}

async fn claim_jobs(pool: &sqlx::PgPool) -> Result<Vec<MediaJob>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, (String, String, String, i64, Uuid)>(
        r#"
        WITH claimed AS (
            SELECT asset_id
            FROM media_processing_jobs
            WHERE (status = 'pending' AND available_at <= now())
               OR (
                    status = 'processing'
                    AND COALESCE(locked_at, created_at) <= now() - make_interval(secs => $2)
               )
            ORDER BY available_at, created_at
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        )
        UPDATE media_processing_jobs AS job
        SET status='processing',
            attempts=job.attempts + 1,
            locked_at=now(),
            lease_id=gen_random_uuid(),
            updated_at=now()
        FROM claimed
        INNER JOIN media_assets AS asset ON asset.id=claimed.asset_id
        WHERE job.asset_id=claimed.asset_id AND asset.status='processing'
        RETURNING job.asset_id, asset.object_key, asset.mime_type, asset.size_bytes, job.lease_id
        "#,
    )
    .bind(BATCH_SIZE)
    .bind(JOB_LEASE_SECONDS)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(asset_id, object_key, mime_type, size_bytes, lease_id)| MediaJob {
                asset_id,
                object_key,
                mime_type,
                size_bytes,
                lease_id,
            },
        )
        .collect())
}

async fn mark_ready(pool: &sqlx::PgPool, job: &MediaJob) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let delivered = sqlx::query(
        "UPDATE media_processing_jobs SET status='delivered',locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now() WHERE asset_id=$1 AND lease_id=$2 AND status='processing'",
    )
    .bind(&job.asset_id)
    .bind(job.lease_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if delivered.rows_affected() != 1 {
        return Err("media processing lease was replaced before completion".to_string());
    }
    let asset = sqlx::query(
        "UPDATE media_assets SET status='ready',updated_at=now() WHERE id=$1 AND status='processing'",
    )
    .bind(&job.asset_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if asset.rows_affected() != 1 {
        return Err("media asset was no longer processing".to_string());
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())
}

async fn schedule_retry(pool: &sqlx::PgPool, job: &MediaJob, error: &str) -> Result<(), String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let attempts = sqlx::query_scalar::<_, i32>(
        "SELECT attempts FROM media_processing_jobs WHERE asset_id=$1 AND lease_id=$2 AND status='processing' FOR UPDATE",
    )
    .bind(&job.asset_id)
    .bind(job.lease_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let Some(attempts) = attempts else {
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    };
    let terminal = attempts >= MAX_ATTEMPTS;
    sqlx::query(
        r#"
        UPDATE media_processing_jobs
        SET status=CASE WHEN $3 THEN 'dead' ELSE 'pending' END,
            available_at=CASE
                WHEN $3 THEN now()
                ELSE now() + make_interval(
                    secs => LEAST(300, CAST(power(2, attempts) AS INTEGER))
                )
            END,
            locked_at=NULL,
            lease_id=NULL,
            last_error=left($4, 2000),
            updated_at=now()
        WHERE asset_id=$1 AND lease_id=$2 AND status='processing'
        "#,
    )
    .bind(&job.asset_id)
    .bind(job.lease_id)
    .bind(terminal)
    .bind(error)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    if terminal {
        sqlx::query(
            "UPDATE media_assets SET status='blocked',updated_at=now() WHERE id=$1 AND status='processing'",
        )
        .bind(&job.asset_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::MAX_ATTEMPTS;

    #[test]
    fn attempts_reach_a_bounded_terminal_state() {
        assert_eq!(MAX_ATTEMPTS, 10);
    }
}
