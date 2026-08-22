use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

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
pub(crate) enum DaoError {
    #[error("media asset was not found")]
    NotFound,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait MediaDao: Send + Sync {
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, DaoError>;
    async fn pending(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError>;
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError>;
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError>;
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, DaoError>;
    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, DaoError>;
}
pub(crate) type SharedMediaDao = Arc<dyn MediaDao>;

async fn load(
    pool: &sqlx::PgPool,
    id: &str,
    owner: Option<&str>,
    status: Option<&str>,
) -> Result<pb::MediaResource, DaoError> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String, Option<i32>, Option<i32>, Option<i64>)>("SELECT object_key,mime_type,size_bytes,status,cdn_url,width,height,duration_ms FROM media_assets WHERE id=$1 AND ($2::text IS NULL OR owner_id=$2) AND ($3::text IS NULL OR status=$3) AND status <> 'deleted'").bind(id).bind(owner).bind(status).fetch_optional(pool).await.map_err(DaoError::Database)?.ok_or(DaoError::NotFound)?;
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
) -> Result<pb::MediaResource, DaoError> {
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
    .map_err(DaoError::Database)?
    .ok_or(DaoError::NotFound)?;
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
    use super::{DaoError, MediaDao, MemoryMediaDao, NewMedia};

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
        let dao = MemoryMediaDao::default();
        dao.create(media("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b", "author-a"))
            .await
            .expect("create asset");
        dao.mark_processing("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b")
            .await
            .expect("local processing completes");
        let repeated_completion = dao
            .mark_processing("0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b")
            .await
            .expect("repeated completion is safe in local storage");
        assert_eq!(repeated_completion.status, "ready");

        let owned = dao
            .owned_ready_batch(
                "author-a",
                &["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string()],
            )
            .await
            .expect("owner can attach their ready asset");
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].status, "ready");

        assert!(matches!(
            dao.owned_ready_batch(
                "author-b",
                &["0184c5bb-76e7-7c77-8d0d-7a03e1d2a13b".to_string()],
            )
            .await,
            Err(DaoError::NotFound)
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

#[path = "memory_media_dao.rs"]
mod memory_media_dao;
pub(crate) use memory_media_dao::MemoryMediaDao;
#[path = "postgres_media_dao.rs"]
mod postgres_media_dao;
pub(crate) use postgres_media_dao::PostgresMediaDao;
#[path = "object_storage.rs"]
mod object_storage;
pub(crate) use object_storage::ObjectStorage;
