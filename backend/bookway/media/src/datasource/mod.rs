use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::pb;

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
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, RepositoryError>;
    async fn pending(&self, id: &str, owner_id: &str)
    -> Result<pb::MediaResource, RepositoryError>;
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError>;
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError>;
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, RepositoryError>;
    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, RepositoryError>;
}
pub(crate) type SharedMediaRepository = Arc<dyn MediaRepository>;

#[derive(Default)]
pub(crate) struct MemoryMediaRepository {
    assets: RwLock<HashMap<String, (String, pb::MediaResource)>>,
}
#[async_trait]
impl MediaRepository for MemoryMediaRepository {
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, RepositoryError> {
        let response = to_response(&media, "pending");
        self.assets
            .write()
            .await
            .insert(media.id, (media.owner_id, response.clone()));
        Ok(response)
    }
    async fn pending(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<pb::MediaResource, RepositoryError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id && media.status == "pending")
            .map(|(_, media)| media.clone())
            .ok_or(RepositoryError::NotFound)
    }
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, _)| owner == owner_id)
            .map(|(_, media)| media.clone())
            .ok_or(RepositoryError::NotFound)
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id || media.status == "ready")
            .map(|(_, media)| media.clone())
            .ok_or(RepositoryError::NotFound)
    }
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, RepositoryError> {
        let mut assets = self.assets.write().await;
        let (_, media) = assets.get_mut(id).ok_or(RepositoryError::NotFound)?;
        // Memory storage has no independently running processor. It is the
        // deterministic local executor for the same already-validated asset.
        media.status = "ready".to_string();
        Ok(media.clone())
    }

    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, RepositoryError> {
        let assets = self.assets.read().await;
        ids.iter()
            .map(|id| {
                assets
                    .get(id)
                    .filter(|(owner, media)| owner == owner_id && media.status == "ready")
                    .map(|(_, media)| media.clone())
                    .ok_or(RepositoryError::NotFound)
            })
            .collect()
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
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, RepositoryError> {
        sqlx::query("INSERT INTO media_assets (id,owner_id,object_key,bucket,mime_type,size_bytes,cdn_url) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(&media.id).bind(&media.owner_id).bind(&media.object_key).bind(&media.bucket).bind(&media.mime_type).bind(i64::try_from(media.size_bytes).unwrap_or(i64::MAX)).bind(&media.cdn_url).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        Ok(to_response(&media, "pending"))
    }
    async fn pending(
        &self,
        id: &str,
        owner_id: &str,
    ) -> Result<pb::MediaResource, RepositoryError> {
        load(&self.pool, id, Some(owner_id), Some("pending")).await
    }
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError> {
        load(&self.pool, id, Some(owner_id), None).await
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, RepositoryError> {
        load_visible(&self.pool, id, owner_id).await
    }
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let row = sqlx::query_as::<_, (String, String, i64, String, Option<i32>, Option<i32>, Option<i64>)>(
            "UPDATE media_assets SET status='processing',updated_at=now() WHERE id=$1 AND status='pending' RETURNING object_key,mime_type,size_bytes,cdn_url,width,height,duration_ms",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let Some(row) = row else {
            transaction
                .commit()
                .await
                .map_err(RepositoryError::Database)?;
            return load(&self.pool, id, None, None).await;
        };
        sqlx::query(
            "INSERT INTO media_processing_jobs (asset_id) VALUES ($1) ON CONFLICT (asset_id) DO UPDATE SET status='pending',available_at=now(),locked_at=NULL,lease_id=NULL,last_error=NULL,updated_at=now()",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(media_response(id, row, "processing"))
    }

    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String, String, i64, String, Option<i32>, Option<i32>, Option<i64>)>(
            "SELECT id,object_key,mime_type,size_bytes,cdn_url,width,height,duration_ms FROM media_assets WHERE owner_id=$1 AND status='ready' AND id = ANY($2) AND status <> 'deleted'",
        )
        .bind(owner_id)
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let by_id = rows
            .into_iter()
            .map(
                |(id, object_key, mime_type, size_bytes, cdn_url, width, height, duration_ms)| {
                    (
                        id.clone(),
                        pb::MediaResource {
                            id,
                            object_key,
                            mime_type,
                            size_bytes: size_bytes.max(0) as u64,
                            status: "ready".to_string(),
                            cdn_url,
                            width: width.unwrap_or_default().max(0) as u32,
                            height: height.unwrap_or_default().max(0) as u32,
                            duration_ms: duration_ms.map(|value| value.max(0) as u64),
                        },
                    )
                },
            )
            .collect::<HashMap<_, _>>();
        ids.iter()
            .map(|id| by_id.get(id).cloned().ok_or(RepositoryError::NotFound))
            .collect()
    }
}

async fn load(
    pool: &sqlx::PgPool,
    id: &str,
    owner: Option<&str>,
    status: Option<&str>,
) -> Result<pb::MediaResource, RepositoryError> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String, Option<i32>, Option<i32>, Option<i64>)>("SELECT object_key,mime_type,size_bytes,status,cdn_url,width,height,duration_ms FROM media_assets WHERE id=$1 AND ($2::text IS NULL OR owner_id=$2) AND ($3::text IS NULL OR status=$3) AND status <> 'deleted'").bind(id).bind(owner).bind(status).fetch_optional(pool).await.map_err(RepositoryError::Database)?.ok_or(RepositoryError::NotFound)?;
    Ok(pb::MediaResource {
        id: id.to_string(),
        object_key: row.0,
        mime_type: row.1,
        size_bytes: row.2.max(0) as u64,
        status: row.3,
        cdn_url: row.4,
        width: row.5.unwrap_or_default().max(0) as u32,
        height: row.6.unwrap_or_default().max(0) as u32,
        duration_ms: row.7.map(|value| value.max(0) as u64),
    })
}

async fn load_visible(
    pool: &sqlx::PgPool,
    id: &str,
    owner_id: &str,
) -> Result<pb::MediaResource, RepositoryError> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            i64,
            String,
            String,
            Option<i32>,
            Option<i32>,
            Option<i64>,
        ),
    >(
        "SELECT object_key,mime_type,size_bytes,status,cdn_url,width,height,duration_ms FROM media_assets WHERE id=$1 AND (owner_id=$2 OR status='ready') AND status <> 'deleted'",
    )
    .bind(id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await
    .map_err(RepositoryError::Database)?
    .ok_or(RepositoryError::NotFound)?;
    Ok(pb::MediaResource {
        id: id.to_string(),
        object_key: row.0,
        mime_type: row.1,
        size_bytes: row.2.max(0) as u64,
        status: row.3,
        cdn_url: row.4,
        width: row.5.unwrap_or_default().max(0) as u32,
        height: row.6.unwrap_or_default().max(0) as u32,
        duration_ms: row.7.map(|value| value.max(0) as u64),
    })
}
fn to_response(media: &NewMedia, status: &str) -> pb::MediaResource {
    pb::MediaResource {
        id: media.id.clone(),
        object_key: media.object_key.clone(),
        mime_type: media.mime_type.clone(),
        size_bytes: media.size_bytes,
        status: status.to_string(),
        cdn_url: media.cdn_url.clone(),
        width: 0,
        height: 0,
        duration_ms: None,
    }
}

fn media_response(
    id: &str,
    (object_key, mime_type, size_bytes, cdn_url, width, height, duration_ms): (
        String,
        String,
        i64,
        String,
        Option<i32>,
        Option<i32>,
        Option<i64>,
    ),
    status: &str,
) -> pb::MediaResource {
    pb::MediaResource {
        id: id.to_string(),
        object_key,
        mime_type,
        size_bytes: size_bytes.max(0) as u64,
        status: status.to_string(),
        cdn_url,
        width: width.unwrap_or_default().max(0) as u32,
        height: height.unwrap_or_default().max(0) as u32,
        duration_ms: duration_ms.map(|value| value.max(0) as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaRepository, MemoryMediaRepository, NewMedia, RepositoryError};

    fn media(id: &str, owner_id: &str) -> NewMedia {
        NewMedia {
            id: id.to_string(),
            owner_id: owner_id.to_string(),
            object_key: format!("{owner_id}/asset"),
            bucket: "bookway-media".to_string(),
            mime_type: "image/jpeg".to_string(),
            size_bytes: 128,
            cdn_url: "https://cdn.example/asset".to_string(),
        }
    }

    #[tokio::test]
    async fn owned_ready_batch_never_leaks_another_users_asset() {
        let repository = MemoryMediaRepository::default();
        repository
            .create(media("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b", "author-a"))
            .await
            .expect("create asset");
        repository
            .mark_processing("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b")
            .await
            .expect("local processing completes");
        let repeated_completion = repository
            .mark_processing("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b")
            .await
            .expect("repeated completion is safe in local storage");
        assert_eq!(repeated_completion.status, "ready");

        let owned = repository
            .owned_ready_batch(
                "author-a",
                &["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string()],
            )
            .await
            .expect("owner can attach their ready asset");
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].status, "ready");

        assert!(matches!(
            repository
                .owned_ready_batch(
                    "author-b",
                    &["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string()],
                )
                .await,
            Err(RepositoryError::NotFound)
        ));
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
