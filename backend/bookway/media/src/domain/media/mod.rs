use std::collections::HashSet;

use uuid::Uuid;

use crate::{
    api::pb,
    datasource::{DaoError, NewMedia},
    domain::{Domain, MediaError},
};

impl Domain {
    pub(crate) async fn create_upload(
        &self,
        request: pb::CreateUploadRequest,
    ) -> Result<pb::UploadResponse, MediaError> {
        let owner = &request.user_id;
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
        self.dao
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
        Ok(pb::UploadResponse {
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
    ) -> Result<pb::MediaResource, MediaError> {
        if !self.proxy_upload {
            return Err(MediaError::Forbidden);
        }
        let media = self.dao.pending(id, owner).await?;
        if body.len() as u64 != media.size_bytes {
            return Err(MediaError::Validation("上传大小与声明不一致".to_string()));
        }
        self.objects
            .upload(&media.object_key, &media.mime_type, body)
            .await?;
        // Byte presence and declared metadata are checked synchronously. The
        // asset cannot be attached to content until the durable processor has
        // finished its additional integrity/audit pass.
        Ok(self.dao.mark_processing(id).await?)
    }
    pub(crate) async fn complete_upload(
        &self,
        request: pb::ResourceRequest,
    ) -> Result<pb::MediaResource, MediaError> {
        let owner = &request.user_id;
        let id = &request.id;
        let media = match self.dao.pending(id, owner).await {
            Ok(media) => media,
            Err(DaoError::NotFound) => {
                // Completion can be retried after a timeout. Once ownership
                // has been established by `get`, returning the existing
                // terminal/intermediate state is safer than creating a
                // second processing job or falsely reporting a missing file.
                return Ok(self.dao.owned(id, owner).await?);
            }
            Err(error) => return Err(error.into()),
        };
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
        Ok(self.dao.mark_processing(id).await?)
    }
    pub(crate) async fn get(
        &self,
        request: pb::ResourceRequest,
    ) -> Result<pb::MediaResource, MediaError> {
        Ok(self.dao.get(&request.id, &request.user_id).await?)
    }

    pub(crate) async fn owned_ready_batch(
        &self,
        request: pb::OwnedReadyMediaRequest,
    ) -> Result<pb::OwnedReadyMediaResponse, MediaError> {
        if request.user_id.trim().is_empty() {
            return Err(MediaError::Validation("媒体所有者不能为空".to_string()));
        }
        if request.ids.is_empty() || request.ids.len() > 12 {
            return Err(MediaError::Validation(
                "一次只能校验 1 到 12 个媒体资源".to_string(),
            ));
        }
        let mut seen = HashSet::with_capacity(request.ids.len());
        let ids = request
            .ids
            .iter()
            .map(|value| value.trim().to_string())
            .map(|id| {
                if Uuid::parse_str(&id).is_err() || !seen.insert(id.clone()) {
                    Err(MediaError::Validation("媒体资源 ID 无效或重复".to_string()))
                } else {
                    Ok(id)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(pb::OwnedReadyMediaResponse {
            items: self
                .dao
                .owned_ready_batch(request.user_id.trim(), &ids)
                .await?,
        })
    }
}
