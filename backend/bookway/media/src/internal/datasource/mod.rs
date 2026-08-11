use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::MediaResponse;

#[derive(Clone)]
pub(crate) struct NewMedia {
    pub(crate) id: String,
    pub(crate) owner_id: String,
    pub(crate) object_key: String,
    pub(crate) bucket: String,
    pub(crate) mime_type: String,
    pub(crate) size_bytes: u64,
    pub(crate) cdn_url: String,
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("media asset was not found")]
    NotFound,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait MediaRepository: Send + Sync {
    async fn create(&self, media: NewMedia) -> Result<MediaResponse, RepositoryError>;
    async fn pending(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError>;
    async fn get(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError>;
    async fn mark_ready(&self, id: &str) -> Result<MediaResponse, RepositoryError>;
}
pub(crate) type SharedMediaRepository = Arc<dyn MediaRepository>;

#[derive(Default)]
pub(crate) struct MemoryMediaRepository {
    assets: RwLock<HashMap<String, (String, MediaResponse)>>,
}
#[async_trait]
impl MediaRepository for MemoryMediaRepository {
    async fn create(&self, media: NewMedia) -> Result<MediaResponse, RepositoryError> {
        let response = to_response(&media, "pending");
        self.assets
            .write()
            .await
            .insert(media.id, (media.owner_id, response.clone()));
        Ok(response)
    }
    async fn pending(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id && media.status == "pending")
            .map(|(_, media)| media.clone())
            .ok_or(RepositoryError::NotFound)
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id || media.status == "ready")
            .map(|(_, media)| media.clone())
            .ok_or(RepositoryError::NotFound)
    }
    async fn mark_ready(&self, id: &str) -> Result<MediaResponse, RepositoryError> {
        let mut assets = self.assets.write().await;
        let (_, media) = assets.get_mut(id).ok_or(RepositoryError::NotFound)?;
        media.status = "ready".to_string();
        Ok(media.clone())
    }
}

pub(crate) struct PostgresMediaRepository {
    pool: sqlx::PgPool,
}
impl PostgresMediaRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl MediaRepository for PostgresMediaRepository {
    async fn create(&self, media: NewMedia) -> Result<MediaResponse, RepositoryError> {
        sqlx::query("INSERT INTO media_assets (id,owner_id,object_key,bucket,mime_type,size_bytes,cdn_url) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(&media.id).bind(&media.owner_id).bind(&media.object_key).bind(&media.bucket).bind(&media.mime_type).bind(i64::try_from(media.size_bytes).unwrap_or(i64::MAX)).bind(&media.cdn_url).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        Ok(to_response(&media, "pending"))
    }
    async fn pending(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError> {
        load(&self.pool, id, Some(owner_id), Some("pending")).await
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<MediaResponse, RepositoryError> {
        load_visible(&self.pool, id, owner_id).await
    }
    async fn mark_ready(&self, id: &str) -> Result<MediaResponse, RepositoryError> {
        let row = sqlx::query_as::<_, (String, String, i64, String)>("UPDATE media_assets SET status='ready',updated_at=now() WHERE id=$1 RETURNING object_key,mime_type,size_bytes,cdn_url").bind(id).fetch_optional(&self.pool).await.map_err(RepositoryError::Database)?.ok_or(RepositoryError::NotFound)?;
        Ok(MediaResponse {
            id: id.to_string(),
            object_key: row.0,
            mime_type: row.1,
            size_bytes: row.2.max(0) as u64,
            status: "ready".to_string(),
            cdn_url: row.3,
        })
    }
}

async fn load(
    pool: &sqlx::PgPool,
    id: &str,
    owner: Option<&str>,
    status: Option<&str>,
) -> Result<MediaResponse, RepositoryError> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String)>("SELECT object_key,mime_type,size_bytes,status,cdn_url FROM media_assets WHERE id=$1 AND ($2::text IS NULL OR owner_id=$2) AND ($3::text IS NULL OR status=$3) AND status <> 'deleted'").bind(id).bind(owner).bind(status).fetch_optional(pool).await.map_err(RepositoryError::Database)?.ok_or(RepositoryError::NotFound)?;
    Ok(MediaResponse {
        id: id.to_string(),
        object_key: row.0,
        mime_type: row.1,
        size_bytes: row.2.max(0) as u64,
        status: row.3,
        cdn_url: row.4,
    })
}

async fn load_visible(
    pool: &sqlx::PgPool,
    id: &str,
    owner_id: &str,
) -> Result<MediaResponse, RepositoryError> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String)>(
        "SELECT object_key,mime_type,size_bytes,status,cdn_url FROM media_assets WHERE id=$1 AND (owner_id=$2 OR status='ready') AND status <> 'deleted'",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or(RepositoryError::NotFound)?;
    Ok(MediaResponse {
        id: id.to_string(),
        object_key: row.0,
        mime_type: row.1,
        size_bytes: row.2.max(0) as u64,
        status: row.3,
        cdn_url: row.4,
    })
}
fn to_response(media: &NewMedia, status: &str) -> MediaResponse {
    MediaResponse {
        id: media.id.clone(),
        object_key: media.object_key.clone(),
        mime_type: media.mime_type.clone(),
        size_bytes: media.size_bytes,
        status: status.to_string(),
        cdn_url: media.cdn_url.clone(),
    }
}

#[derive(Debug, Error)]
pub(crate) enum ObjectError {
    #[error("invalid S3 endpoint: {0}")]
    Url(#[from] url::ParseError),
    #[error("invalid S3 bucket configuration: {0}")]
    Bucket(String),
    #[error("object storage request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("object storage rejected the request")]
    Rejected,
    #[error("object storage returned invalid metadata: {0}")]
    InvalidMetadata(String),
}

pub(crate) struct ObjectMetadata {
    pub(crate) size_bytes: u64,
    pub(crate) mime_type: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ObjectStorage {
    bucket: Bucket,
    credentials: Credentials,
    client: reqwest::Client,
}
impl ObjectStorage {
    pub(crate) fn new(
        endpoint: &str,
        bucket: String,
        region: String,
        key: String,
        secret: String,
    ) -> Result<Self, ObjectError> {
        let bucket = Bucket::new(endpoint.parse()?, UrlStyle::Path, bucket, region)
            .map_err(|error| ObjectError::Bucket(error.to_string()))?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(key, secret),
            client: bookway_runtime::http_client(),
        })
    }
    pub(crate) fn presign_put(&self, key: &str) -> String {
        self.bucket
            .put_object(Some(&self.credentials), key)
            .sign(Duration::from_secs(900))
            .to_string()
    }
    pub(crate) async fn metadata(&self, key: &str) -> Result<Option<ObjectMetadata>, ObjectError> {
        let response = self
            .client
            .head(
                self.bucket
                    .head_object(Some(&self.credentials), key)
                    .sign(Duration::from_secs(60)),
            )
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response.error_for_status()?;
        let size_bytes = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| ObjectError::InvalidMetadata("missing content-length".to_string()))?;
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
            });
        Ok(Some(ObjectMetadata {
            size_bytes,
            mime_type,
        }))
    }
    pub(crate) async fn upload(
        &self,
        key: &str,
        mime: &str,
        body: axum::body::Bytes,
    ) -> Result<(), ObjectError> {
        let response = self
            .client
            .put(self.presign_put(key))
            .header("content-type", mime)
            .body(body)
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(ObjectError::Rejected)
        }
    }
}
