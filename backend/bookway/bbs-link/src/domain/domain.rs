use std::sync::Arc;

use bookway_content_audit_api::pb::{self as audit_pb, content_audit_client::ContentAuditClient};
use bookway_media_api::pb::{self as media_pb, media_client::MediaClient};
use thiserror::Error;

use crate::{
    conf::Config,
    datasource::{
        ContentDao, MemoryContentDao, PostgresContentDao, DaoError,
    },
};

#[derive(Debug, Error)]
pub(crate) enum ContentError {
    #[error("{0}")]
    Validation(String),
    #[error("content belongs to another author")]
    Forbidden,
    #[error(transparent)]
    Dao(#[from] DaoError),
    #[error("content audit unavailable: {0}")]
    Audit(String),
    #[error("media validation unavailable: {0}")]
    Media(String),
}

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) Dao: Arc<dyn ContentDao>,
    pub(crate) content_audit: Option<ContentAuditClient<tonic::transport::Channel>>,
    pub(crate) media: Option<MediaClient<tonic::transport::Channel>>,
}

impl Domain {
    pub(crate) async fn new(
        config: Config,
        media: Option<MediaClient<tonic::transport::Channel>>,
    ) -> Result<Self, bookway_data::DataError> {
        let Dao: Arc<dyn ContentDao> = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Memory => Arc::new(MemoryContentDao::seeded()),
            bookway_data::StorageMode::Postgres => Arc::new(PostgresContentDao::new(
                bookway_data::postgres_pool().await?,
            )),
        };
        let content_audit = match config.content_audit_grpc_url.clone() {
            Some(url) => Some(ContentAuditClient::connect(url).await.map_err(|error| {
                bookway_data::DataError::InvalidPoolSetting {
                    key: "CONTENT_AUDIT_GRPC_URL",
                    value: error.to_string(),
                }
            })?),
            None => None,
        };
        Ok(Self {
            config,
            Dao,
            content_audit,
            media,
        })
    }

    pub(crate) async fn audit(
        &self,
        request: audit_pb::AuditRequest,
    ) -> Result<audit_pb::AuditResponse, ContentError> {
        let Some(mut client) = self.content_audit.clone() else {
            return Ok(audit_pb::AuditResponse {
                decision: audit_pb::AuditDecision::Approved as i32,
                risk_score: 0.0,
                reasons: Vec::new(),
                provider: "local-development".to_string(),
            });
        };
        let request = bookway_runtime::grpc_service_request(request)
            .map_err(|error| ContentError::Audit(error.to_string()))?;
        client
            .audit(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(|error| ContentError::Audit(error.to_string()))
    }

    pub(crate) async fn owned_ready_media(
        &self,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<Vec<media_pb::MediaResource>, ContentError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let Some(mut client) = self.media.clone() else {
            return Err(ContentError::Media(
                "media client is not configured".to_string(),
            ));
        };
        let request = bookway_runtime::grpc_service_request(media_pb::OwnedReadyMediaRequest {
            user_id,
            ids: ids.clone(),
        })
        .map_err(|error| ContentError::Media(error.to_string()))?;
        let response = match client.get_owned_ready_batch(request).await {
            Ok(response) => response.into_inner(),
            Err(error) => {
                return Err(match error.code() {
                    tonic::Code::InvalidArgument
                    | tonic::Code::NotFound
                    | tonic::Code::PermissionDenied => ContentError::Validation(
                        "媒体资源无效、尚未处理完成或不属于当前作者".to_string(),
                    ),
                    _ => ContentError::Media(error.to_string()),
                });
            }
        };
        let matches_request = response.items.len() == ids.len()
            && response
                .items
                .iter()
                .zip(&ids)
                .all(|(asset, id)| asset.id == *id && asset.status == "ready");
        if matches_request {
            Ok(response.items)
        } else {
            Err(ContentError::Media(
                "Media returned an invalid owned-ready asset response".to_string(),
            ))
        }
    }

    #[cfg(test)]
    pub(crate) fn from_dao(config: Config, Dao: Arc<dyn ContentDao>) -> Self {
        Self {
            config,
            Dao,
            content_audit: None,
            media: None,
        }
    }
}
