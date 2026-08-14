use async_trait::async_trait;
use thiserror::Error;
use tonic::transport::Channel;

use super::api::{
    ActionDto, CommentDto, ContentDto, CreateActionRequest, CreateCommentRequest,
    CreateContentRequest, CreateJourneyRequest, FeedDto, FeedQueryRequest, FollowRequest,
    JourneyDetailDto, JourneyDto, MediaDto, MediaUploadRequest, MediaUploadResponse, ReactionDto,
    ReactionRequest, SearchQueryRequest, SearchResponseDto, SuggestionResponseDto, TodayDto,
    UpdateActionRequest, UpdateContentRequest, UpdateJourneyRequest, UserEventBatchRequest,
    UserEventIngestResponse,
};

#[derive(Debug, Error)]
pub(crate) enum UpstreamError {
    #[error("{service} grpc request failed: {message}")]
    Transport {
        service: &'static str,
        message: String,
    },
    #[error("{service} grpc request failed with {code:?}: {message}")]
    Grpc {
        service: &'static str,
        code: tonic::Code,
        message: String,
    },
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

enum Client {
    Growth(bookway_growth::api::pb::growth_client::GrowthClient<Channel>),
    BbsFeed(bookway_bbs_feed::api::pb::bbs_feed_client::BbsFeedClient<Channel>),
    BbsLink(bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient<Channel>),
    SearchMain(bookway_search_main::api::pb::search_main_client::SearchMainClient<Channel>),
    Bbs(bookway_bbs::api::pb::bbs_client::BbsClient<Channel>),
    Comment(bookway_comment::api::pb::comment_client::CommentClient<Channel>),
    LikeStatus(
        bookway_commonlikestatus::api::pb::common_like_status_client::CommonLikeStatusClient<
            Channel,
        >,
    ),
    UserEvent(bookway_user_event::api::pb::user_event_client::UserEventClient<Channel>),
    Media(bookway_media::api::pb::media_client::MediaClient<Channel>),
}

pub(crate) struct GrpcDataSource {
    client: Client,
}

impl GrpcDataSource {
    pub(crate) async fn connect(
        service: &'static str,
        address: String,
    ) -> Result<Self, tonic::transport::Error> {
        let client = match service {
            "growth" => Client::Growth(
                bookway_growth::api::pb::growth_client::GrowthClient::connect(address).await?,
            ),
            "bbs-feed" => Client::BbsFeed(
                bookway_bbs_feed::api::pb::bbs_feed_client::BbsFeedClient::connect(address)
                    .await?,
            ),
            "bbs-link" => Client::BbsLink(
                bookway_bbs_link::api::pb::bbs_link_client::BbsLinkClient::connect(address)
                    .await?,
            ),
            "search-main" => Client::SearchMain(
                bookway_search_main::api::pb::search_main_client::SearchMainClient::connect(
                    address,
                )
                .await?,
            ),
            "bbs" => Client::Bbs(
                bookway_bbs::api::pb::bbs_client::BbsClient::connect(address).await?,
            ),
            "comment" => Client::Comment(
                bookway_comment::api::pb::comment_client::CommentClient::connect(address).await?,
            ),
            "commonlikestatus" => Client::LikeStatus(
                bookway_commonlikestatus::api::pb::common_like_status_client::CommonLikeStatusClient::connect(address).await?,
            ),
            "user-event" => Client::UserEvent(
                bookway_user_event::api::pb::user_event_client::UserEventClient::connect(address)
                    .await?,
            ),
            "media" => Client::Media(
                bookway_media::api::pb::media_client::MediaClient::connect(address).await?,
            ),
            _ => panic!("unsupported gateway grpc service: {service}"),
        };
        Ok(Self { client })
    }
}

fn encode<T: serde::Serialize>(service: &'static str, value: &T) -> Result<String, UpstreamError> {
    serde_json::to_string(value).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

fn decode<T: serde::de::DeserializeOwned>(
    service: &'static str,
    value: String,
) -> Result<T, UpstreamError> {
    serde_json::from_str(&value).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

fn status<T>(service: &'static str, result: Result<T, tonic::Status>) -> Result<T, UpstreamError> {
    result.map_err(|error| UpstreamError::Grpc {
        service,
        code: error.code(),
        message: error.to_string(),
    })
}

impl GrpcDataSource {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .list_journeys(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_journey(bookway_growth::api::pb::CreateJourneyRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .get_journey(bookway_growth::api::pb::JourneyRequest {
                    user_id: user_id.to_string(),
                    journey_id: journey_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_journey(bookway_growth::api::pb::UpdateJourneyRequest {
                    user_id: user_id.to_string(),
                    journey_id: journey_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        request: CreateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .create_action(bookway_growth::api::pb::CreateActionRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn today(&self, user_id: &str) -> Result<TodayDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .today(bookway_growth::api::pb::UserRequest {
                    user_id: user_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .complete_action(bookway_growth::api::pb::CompleteActionRequest {
                    user_id: user_id.to_string(),
                    action_id: action_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, UpstreamError> {
        let Client::Growth(client) = &self.client else {
            return Err(wrong_service("growth"));
        };
        let mut client = client.clone();
        let response = status(
            "growth",
            client
                .update_action(bookway_growth::api::pb::UpdateActionRequest {
                    user_id: user_id.to_string(),
                    action_id: action_id.to_string(),
                    request_json: encode("growth", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("growth", response.response_json)
    }
}

#[async_trait]
impl BbsFeedDataSource for GrpcDataSource {
    async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError> {
        let Client::BbsFeed(client) = &self.client else {
            return Err(wrong_service("bbs-feed"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-feed",
            client
                .feed(bookway_bbs_feed::api::pb::FeedRequest {
                    request_json: encode("bbs-feed", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("bbs-feed", response.response_json)
    }
}

#[async_trait]
impl BbsLinkDataSource for GrpcDataSource {
    async fn get(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .get(bookway_bbs_link::api::pb::IdRequest { id: id.to_string() })
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn create(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .create(bookway_bbs_link::api::pb::CreateRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("bbs-link", &request)?,
                    idempotency_key: idempotency_key.unwrap_or_default(),
                })
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn update(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .update(bookway_bbs_link::api::pb::UpdateRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                    request_json: encode("bbs-link", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }

    async fn publish(&self, user_id: &str, id: &str) -> Result<ContentDto, UpstreamError> {
        let Client::BbsLink(client) = &self.client else {
            return Err(wrong_service("bbs-link"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs-link",
            client
                .publish(bookway_bbs_link::api::pb::PublishRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("bbs-link", response.response_json)
    }
}

#[async_trait]
impl SearchMainDataSource for GrpcDataSource {
    async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, UpstreamError> {
        let Client::SearchMain(client) = &self.client else {
            return Err(wrong_service("search-main"));
        };
        let mut client = client.clone();
        let response = status(
            "search-main",
            client
                .search(bookway_search_main::api::pb::SearchRequest {
                    request_json: encode("search-main", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("search-main", response.response_json)
    }

    async fn suggestions(&self, query: &str) -> Result<SuggestionResponseDto, UpstreamError> {
        let Client::SearchMain(client) = &self.client else {
            return Err(wrong_service("search-main"));
        };
        let mut client = client.clone();
        let response = status(
            "search-main",
            client
                .suggestions(bookway_search_main::api::pb::SuggestionsRequest {
                    query: query.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("search-main", response.response_json)
    }
}

#[async_trait]
impl UserEventDataSource for GrpcDataSource {
    async fn ingest(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError> {
        let Client::UserEvent(client) = &self.client else {
            return Err(wrong_service("user-event"));
        };
        let mut client = client.clone();
        let response = status(
            "user-event",
            client
                .ingest(bookway_user_event::api::pb::IngestRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("user-event", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("user-event", response.response_json)
    }
}

#[async_trait]
impl MediaDataSource for GrpcDataSource {
    async fn create_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .create_upload(bookway_media::api::pb::CreateUploadRequest {
                    user_id: user_id.to_string(),
                    request_json: encode("media", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }

    async fn complete_upload(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .complete_upload(bookway_media::api::pb::ResourceRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }

    async fn get(&self, user_id: &str, id: &str) -> Result<MediaDto, UpstreamError> {
        let Client::Media(client) = &self.client else {
            return Err(wrong_service("media"));
        };
        let mut client = client.clone();
        let response = status(
            "media",
            client
                .get(bookway_media::api::pb::ResourceRequest {
                    user_id: user_id.to_string(),
                    id: id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("media", response.response_json)
    }
}

#[async_trait]
impl BbsDataSource for GrpcDataSource {
    async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError> {
        let Client::Bbs(client) = &self.client else {
            return Err(wrong_service("bbs"));
        };
        let mut client = client.clone();
        let response = status(
            "bbs",
            client
                .set_edge(bookway_bbs::api::pb::SetEdgeRequest {
                    user_id: user_id.to_string(),
                    target_user_id: target_user_id.to_string(),
                    request_json: encode("bbs", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("bbs", response.response_json)
    }
}

#[async_trait]
impl CommentDataSource for GrpcDataSource {
    async fn comments(&self, post_id: &str) -> Result<Vec<CommentDto>, UpstreamError> {
        let Client::Comment(client) = &self.client else {
            return Err(wrong_service("comment"));
        };
        let mut client = client.clone();
        let response = status(
            "comment",
            client
                .list(bookway_comment::api::pb::ListRequest {
                    post_id: post_id.to_string(),
                })
                .await,
        )?
        .into_inner();
        decode("comment", response.response_json)
    }

    async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
    ) -> Result<CommentDto, UpstreamError> {
        let Client::Comment(client) = &self.client else {
            return Err(wrong_service("comment"));
        };
        let mut client = client.clone();
        let response = status(
            "comment",
            client
                .create(bookway_comment::api::pb::CreateRequest {
                    user_id: user_id.to_string(),
                    post_id: post_id.to_string(),
                    request_json: encode("comment", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("comment", response.response_json)
    }
}

#[async_trait]
impl LikeStatusDataSource for GrpcDataSource {
    async fn reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError> {
        let Client::LikeStatus(client) = &self.client else {
            return Err(wrong_service("commonlikestatus"));
        };
        let mut client = client.clone();
        let response = status(
            "commonlikestatus",
            client
                .set_reaction(bookway_commonlikestatus::api::pb::SetReactionRequest {
                    user_id: user_id.to_string(),
                    post_id: post_id.to_string(),
                    request_json: encode("commonlikestatus", &request)?,
                })
                .await,
        )?
        .into_inner();
        decode("commonlikestatus", response.response_json)
    }
}

fn wrong_service(service: &'static str) -> UpstreamError {
    UpstreamError::Transport {
        service,
        message: "client type does not match datasource contract".to_string(),
    }
}
