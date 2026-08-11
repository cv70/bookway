use async_trait::async_trait;
use bookway_api::{ApiResponse, ErrorResponse};
use serde::de::DeserializeOwned;
use thiserror::Error;

use super::api::{
    ActionDto, CommentDto, ContentDto, CreateCommentRequest, CreateContentRequest,
    CreateJourneyRequest, FeedDto, FeedQueryRequest, FollowRequest, JourneyDto, MediaDto,
    MediaUploadRequest, MediaUploadResponse, ReactionDto, ReactionRequest, SearchQueryRequest,
    SearchResponseDto, SuggestionResponseDto, TodayDto, UpdateContentRequest,
    UserEventBatchRequest, UserEventIngestResponse,
};

#[derive(Debug, Error)]
pub(crate) enum UpstreamError {
    #[error("{service} request failed: {source}")]
    Transport {
        service: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{service} rejected the request: {message}")]
    Rejected {
        service: &'static str,
        status: u16,
        code: String,
        message: String,
    },
}

#[async_trait]
pub(crate) trait GrowthDataSource: Send + Sync {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<JourneyDto>, UpstreamError>;
    async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError>;
    async fn today(&self, user_id: &str) -> Result<TodayDto, UpstreamError>;
    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait BbsFeedDataSource: Send + Sync {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait BbsLinkDataSource: Send + Sync {
    async fn get(&self, id: &str) -> Result<ContentDto, UpstreamError>;
    async fn create(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError>;
    async fn update(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError>;
    async fn publish(&self, user_id: &str, id: &str) -> Result<ContentDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait SearchMainDataSource: Send + Sync {
    async fn search(&self, request: SearchQueryRequest)
    -> Result<SearchResponseDto, UpstreamError>;
    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait UserEventDataSource: Send + Sync {
    async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError>;
}

#[async_trait]
pub(crate) trait MediaDataSource: Send + Sync {
    async fn create_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError>;
    async fn complete_upload(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError>;
    async fn get(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait BbsDataSource: Send + Sync {
    async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait CommentDataSource: Send + Sync {
    async fn comments(&self, post_id: &str) -> Result<Vec<CommentDto>, UpstreamError>;
    async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
    ) -> Result<CommentDto, UpstreamError>;
}

#[async_trait]
pub(crate) trait LikeStatusDataSource: Send + Sync {
    async fn reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError>;
}

pub(crate) struct HttpGrowthDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpGrowthDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl GrowthDataSource for HttpGrowthDataSource {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<JourneyDto>, UpstreamError> {
        decode(
            "growth",
            self.client
                .get(format!("{}/internal/v1/journeys", self.base_url))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("growth", source))?,
        )
        .await
    }

    async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        decode(
            "growth",
            self.client
                .post(format!("{}/internal/v1/journeys", self.base_url))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("growth", source))?,
        )
        .await
    }

    async fn today(&self, user_id: &str) -> Result<TodayDto, UpstreamError> {
        decode(
            "growth",
            self.client
                .get(format!("{}/internal/v1/today", self.base_url))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("growth", source))?,
        )
        .await
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError> {
        decode(
            "growth",
            self.client
                .post(format!(
                    "{}/internal/v1/actions/{action_id}/complete",
                    self.base_url
                ))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("growth", source))?,
        )
        .await
    }
}

pub(crate) struct HttpBbsFeedDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsFeedDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsFeedDataSource for HttpBbsFeedDataSource {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError> {
        decode(
            "bbs-feed",
            self.client
                .get(format!("{}/internal/v1/feed", self.base_url))
                .query(&request)
                .send()
                .await
                .map_err(|source| transport("bbs-feed", source))?,
        )
        .await
    }
}

pub(crate) struct HttpBbsLinkDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsLinkDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsLinkDataSource for HttpBbsLinkDataSource {
    async fn get(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        decode(
            "bbs-link",
            self.client
                .get(format!("{}/internal/v1/posts/{id}", self.base_url))
                .send()
                .await
                .map_err(|source| transport("bbs-link", source))?,
        )
        .await
    }

    async fn create(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError> {
        let mut builder = self
            .client
            .post(format!("{}/internal/v1/posts", self.base_url))
            .header("x-user-id", user_id)
            .json(&request);
        if let Some(key) = idempotency_key {
            builder = builder.header("idempotency-key", key);
        }
        decode(
            "bbs-link",
            builder
                .send()
                .await
                .map_err(|source| transport("bbs-link", source))?,
        )
        .await
    }

    async fn update(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError> {
        decode(
            "bbs-link",
            self.client
                .patch(format!("{}/internal/v1/posts/{id}", self.base_url))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("bbs-link", source))?,
        )
        .await
    }

    async fn publish(&self, user_id: &str, id: &str) -> Result<ContentDto, UpstreamError> {
        decode(
            "bbs-link",
            self.client
                .post(format!("{}/internal/v1/posts/{id}/publish", self.base_url))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("bbs-link", source))?,
        )
        .await
    }
}

pub(crate) struct HttpSearchMainDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpSearchMainDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl SearchMainDataSource for HttpSearchMainDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, UpstreamError> {
        decode(
            "search-main",
            self.client
                .get(format!("{}/internal/v1/search", self.base_url))
                .query(&request)
                .send()
                .await
                .map_err(|source| transport("search-main", source))?,
        )
        .await
    }

    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, UpstreamError> {
        decode(
            "search-main",
            self.client
                .get(format!("{}/internal/v1/suggestions", self.base_url))
                .query(&[("q", query)])
                .send()
                .await
                .map_err(|source| transport("search-main", source))?,
        )
        .await
    }
}

pub(crate) struct HttpUserEventDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpUserEventDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl UserEventDataSource for HttpUserEventDataSource {
    async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError> {
        decode(
            "user-event",
            self.client
                .post(format!("{}/internal/v1/events", self.base_url))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("user-event", source))?,
        )
        .await
    }
}

pub(crate) struct HttpMediaDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpMediaDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl MediaDataSource for HttpMediaDataSource {
    async fn create_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError> {
        decode(
            "media",
            self.client
                .post(format!("{}/internal/v1/media/upload-url", self.base_url))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("media", source))?,
        )
        .await
    }

    async fn complete_upload(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        decode(
            "media",
            self.client
                .post(format!("{}/internal/v1/media/{id}/complete", self.base_url))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("media", source))?,
        )
        .await
    }

    async fn get(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        decode(
            "media",
            self.client
                .get(format!("{}/internal/v1/media/{id}", self.base_url))
                .header("x-user-id", user_id)
                .send()
                .await
                .map_err(|source| transport("media", source))?,
        )
        .await
    }
}

pub(crate) struct HttpBbsDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpBbsDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl BbsDataSource for HttpBbsDataSource {
    async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError> {
        decode(
            "bbs",
            self.client
                .put(format!(
                    "{}/internal/v1/users/{target_user_id}/follow",
                    self.base_url
                ))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("bbs", source))?,
        )
        .await
    }
}

pub(crate) struct HttpCommentDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpCommentDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl CommentDataSource for HttpCommentDataSource {
    async fn comments(&self, post_id: &str) -> Result<Vec<CommentDto>, UpstreamError> {
        decode(
            "comment",
            self.client
                .get(format!(
                    "{}/internal/v1/posts/{post_id}/comments",
                    self.base_url
                ))
                .send()
                .await
                .map_err(|source| transport("comment", source))?,
        )
        .await
    }

    async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
    ) -> Result<CommentDto, UpstreamError> {
        decode(
            "comment",
            self.client
                .post(format!(
                    "{}/internal/v1/posts/{post_id}/comments",
                    self.base_url
                ))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("comment", source))?,
        )
        .await
    }
}

pub(crate) struct HttpLikeStatusDataSource {
    client: reqwest::Client,
    base_url: String,
}

impl HttpLikeStatusDataSource {
    pub(crate) fn new(base_url: String) -> Self {
        Self {
            client: bookway_runtime::http_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait]
impl LikeStatusDataSource for HttpLikeStatusDataSource {
    async fn reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError> {
        decode(
            "commonlikestatus",
            self.client
                .put(format!(
                    "{}/internal/v1/posts/{post_id}/reactions",
                    self.base_url
                ))
                .header("x-user-id", user_id)
                .json(&request)
                .send()
                .await
                .map_err(|source| transport("commonlikestatus", source))?,
        )
        .await
    }
}

fn transport(service: &'static str, source: reqwest::Error) -> UpstreamError {
    UpstreamError::Transport { service, source }
}

async fn decode<T: DeserializeOwned>(
    service: &'static str,
    response: reqwest::Response,
) -> Result<T, UpstreamError> {
    let status = response.status();
    if status.is_success() {
        return response
            .json::<ApiResponse<T>>()
            .await
            .map(|response| response.data)
            .map_err(|source| transport(service, source));
    }

    let fallback_status = status.as_u16();
    match response.json::<ErrorResponse>().await {
        Ok(error) => Err(UpstreamError::Rejected {
            service,
            status: fallback_status,
            code: error.error.code,
            message: error.error.message,
        }),
        Err(source) => Err(UpstreamError::Transport { service, source }),
    }
}
