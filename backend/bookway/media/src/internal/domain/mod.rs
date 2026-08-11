use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use super::{
    api::{MediaResponse, UploadRequest, UploadResponse},
    datasource::{NewMedia, ObjectError, ObjectStorage, RepositoryError, SharedMediaRepository},
};

#[derive(Debug, Error)]
pub(crate) enum MediaError {
    #[error("{0}")]
    Validation(String),
    #[error("proxy upload is disabled")]
    Forbidden,
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

#[derive(Clone)]
pub(crate) struct MediaService {
    repository: SharedMediaRepository,
    objects: Arc<ObjectStorage>,
    bucket: String,
    cdn_base: String,
    proxy_upload: bool,
}
impl MediaService {
    pub(crate) fn new(
        repository: SharedMediaRepository,
        objects: Arc<ObjectStorage>,
        bucket: String,
        cdn_base: String,
        proxy_upload: bool,
    ) -> Self {
        Self {
            repository,
            objects,
            bucket,
            cdn_base,
            proxy_upload,
        }
    }
    pub(crate) async fn create_upload(
        &self,
        owner: &str,
        request: UploadRequest,
    ) -> Result<UploadResponse, MediaError> {
        if request.size_bytes == 0 || request.size_bytes > 512 * 1024 * 1024 {
            return Err(MediaError::Validation(
                "文件大小必须在 1B 到 512MB 之间".to_string(),
            ));
        }
        if !matches!(
            request.mime_type.as_str(),
            "image/jpeg" | "image/png" | "image/webp" | "video/mp4" | "audio/mpeg"
        ) {
            return Err(MediaError::Validation("不支持的媒体类型".to_string()));
        }
        let id = Uuid::now_v7().to_string();
        let object_key = format!("{}/{}/{}", owner, &id[..2], id);
        let cdn_url = format!("{}/{}", self.cdn_base, object_key);
        self.repository
            .create(NewMedia {
                id: id.clone(),
                owner_id: owner.to_string(),
                object_key: object_key.clone(),
                bucket: self.bucket.clone(),
                mime_type: request.mime_type,
                size_bytes: request.size_bytes,
                cdn_url: cdn_url.clone(),
            })
            .await?;
        Ok(UploadResponse {
            id,
            upload_url: self.objects.presign_put(&object_key),
            object_key,
            cdn_url,
            expires_in_seconds: 900,
        })
    }
    pub(crate) async fn proxy_upload(
        &self,
        owner: &str,
        id: &str,
        body: axum::body::Bytes,
    ) -> Result<MediaResponse, MediaError> {
        if !self.proxy_upload {
            return Err(MediaError::Forbidden);
        }
        let media = self.repository.pending(id, owner).await?;
        if body.len() as u64 != media.size_bytes {
            return Err(MediaError::Validation("上传大小与声明不一致".to_string()));
        }
        self.objects
            .upload(&media.object_key, &media.mime_type, body)
            .await?;
        Ok(self.repository.mark_ready(id).await?)
    }
    pub(crate) async fn complete_upload(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<MediaResponse, MediaError> {
        let media = self.repository.pending(id, owner).await?;
        let Some(metadata) = self.objects.metadata(&media.object_key).await? else {
            return Err(MediaError::Validation("对象尚未上传完成".to_string()));
        };
        if metadata.size_bytes != media.size_bytes {
            return Err(MediaError::Validation(format!(
                "对象大小与声明不一致：期望 {}B，实际 {}B",
                media.size_bytes, metadata.size_bytes
            )));
        }
        if metadata.mime_type.as_deref() != Some(media.mime_type.as_str()) {
            return Err(MediaError::Validation(format!(
                "对象 MIME 与声明不一致：期望 {}，实际 {}",
                media.mime_type,
                metadata.mime_type.as_deref().unwrap_or("unknown")
            )));
        }
        Ok(self.repository.mark_ready(id).await?)
    }
    pub(crate) async fn get(&self, owner: &str, id: &str) -> Result<MediaResponse, MediaError> {
        Ok(self.repository.get(id, owner).await?)
    }
}
