use std::sync::Arc;

use crate::{conf::Config, datasource::GrpcDataSource};

use super::{
    api::{
        ActionDto, CommentDto, ContentDto, CreateCommentRequest, CreateContentRequest,
        CreateJourneyRequest, FeedDto, FeedQueryRequest, FollowRequest, JourneyDto, MediaDto,
        MediaUploadRequest, MediaUploadResponse, ReactionDto, ReactionRequest, SearchQueryRequest,
        SearchResponseDto, SuggestionResponseDto, TodayDto, UpdateContentRequest,
        UserEventBatchRequest, UserEventIngestResponse,
    },
    datasource::{
        BbsDataSource, BbsFeedDataSource, BbsLinkDataSource, CommentDataSource,
        LikeStatusDataSource, MediaDataSource, SearchMainDataSource, UpstreamError,
        UserEventDataSource,
    },
};

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    pub(crate) growth: Arc<GrpcDataSource>,
    pub(crate) bbs_feed: Arc<dyn BbsFeedDataSource>,
    pub(crate) bbs_link: Arc<dyn BbsLinkDataSource>,
    pub(crate) search_main: Arc<dyn SearchMainDataSource>,
    pub(crate) bbs: Arc<dyn BbsDataSource>,
    pub(crate) comment: Arc<dyn CommentDataSource>,
    pub(crate) like_status: Arc<dyn LikeStatusDataSource>,
    pub(crate) user_event: Arc<dyn UserEventDataSource>,
    pub(crate) media: Arc<dyn MediaDataSource>,
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let growth = Arc::new(GrpcDataSource::connect("growth", config.growth_url.clone()).await?);
        let bbs_feed =
            Arc::new(GrpcDataSource::connect("bbs-feed", config.bbs_feed_url.clone()).await?);
        let bbs_link =
            Arc::new(GrpcDataSource::connect("bbs-link", config.bbs_link_url.clone()).await?);
        let search_main =
            Arc::new(GrpcDataSource::connect("search-main", config.search_main_url.clone()).await?);
        let bbs = Arc::new(GrpcDataSource::connect("bbs", config.bbs_url.clone()).await?);
        let comment =
            Arc::new(GrpcDataSource::connect("comment", config.comment_url.clone()).await?);
        let like_status = Arc::new(
            GrpcDataSource::connect("commonlikestatus", config.like_status_url.clone()).await?,
        );
        let user_event =
            Arc::new(GrpcDataSource::connect("user-event", config.user_event_url.clone()).await?);
        let media = Arc::new(GrpcDataSource::connect("media", config.media_url.clone()).await?);
        Ok(Self {
            config,
            growth,
            bbs_feed,
            bbs_link,
            search_main,
            bbs,
            comment,
            like_status,
            user_event,
            media,
        })
    }
}

impl Domain {
    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, UpstreamError> {
        self.growth.list_journeys(user_id).await
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, UpstreamError> {
        self.growth.create_journey(user_id, request).await
    }

    pub(crate) async fn today(&self, user_id: &str) -> Result<TodayDto, UpstreamError> {
        self.growth.today(user_id).await
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, UpstreamError> {
        self.growth.complete_action(user_id, action_id).await
    }

    pub(crate) async fn feed(&self, request: FeedQueryRequest) -> Result<FeedDto, UpstreamError> {
        self.bbs_feed.feed(request).await
    }

    pub(crate) async fn get_content(&self, id: &str) -> Result<ContentDto, UpstreamError> {
        self.bbs_link.get(id).await
    }

    pub(crate) async fn create_content(
        &self,
        user_id: &str,
        request: CreateContentRequest,
        idempotency_key: Option<String>,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link
            .create(user_id, request, idempotency_key)
            .await
    }

    pub(crate) async fn update_content(
        &self,
        user_id: &str,
        id: &str,
        request: UpdateContentRequest,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link.update(user_id, id, request).await
    }

    pub(crate) async fn publish_content(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<ContentDto, UpstreamError> {
        self.bbs_link.publish(user_id, id).await
    }

    pub(crate) async fn search(
        &self,
        request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, UpstreamError> {
        self.search_main.search(request).await
    }

    pub(crate) async fn suggestions(
        &self,
        query: &str,
    ) -> Result<SuggestionResponseDto, UpstreamError> {
        self.search_main.suggestions(query).await
    }

    pub(crate) async fn ingest_events(
        &self,
        user_id: &str,
        request: UserEventBatchRequest,
    ) -> Result<UserEventIngestResponse, UpstreamError> {
        self.user_event.ingest(user_id, request).await
    }

    pub(crate) async fn create_media_upload(
        &self,
        user_id: &str,
        request: MediaUploadRequest,
    ) -> Result<MediaUploadResponse, UpstreamError> {
        self.media.create_upload(user_id, request).await
    }

    pub(crate) async fn complete_media_upload(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<MediaDto, UpstreamError> {
        self.media.complete_upload(user_id, id).await
    }

    pub(crate) async fn get_media(
        &self,
        user_id: &str,
        id: &str,
    ) -> Result<MediaDto, UpstreamError> {
        self.media.get(user_id, id).await
    }

    pub(crate) async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        request: ReactionRequest,
    ) -> Result<ReactionDto, UpstreamError> {
        let _ = self.bbs_link.get(post_id).await?;
        self.like_status.reaction(user_id, post_id, request).await
    }

    pub(crate) async fn comments(&self, post_id: &str) -> Result<Vec<CommentDto>, UpstreamError> {
        let _ = self.bbs_link.get(post_id).await?;
        self.comment.comments(post_id).await
    }

    pub(crate) async fn create_comment(
        &self,
        user_id: &str,
        post_id: &str,
        request: CreateCommentRequest,
    ) -> Result<CommentDto, UpstreamError> {
        let _ = self.bbs_link.get(post_id).await?;
        self.comment.create_comment(user_id, post_id, request).await
    }

    pub(crate) async fn follow(
        &self,
        user_id: &str,
        target_user_id: &str,
        request: FollowRequest,
    ) -> Result<bookway_api::SocialContextDto, UpstreamError> {
        self.bbs.follow(user_id, target_user_id, request).await
    }
}
