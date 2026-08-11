use std::sync::Arc;

use super::{
    api::{
        ActionDto, CommentDto, ContentDto, CreateCommentRequest, CreateContentRequest,
        CreateJourneyRequest, FeedDto, FeedQueryRequest, FollowRequest, JourneyDto, MediaDto,
        MediaUploadRequest, MediaUploadResponse, ReactionDto, ReactionRequest, SearchQueryRequest,
        SearchResponseDto, SuggestionResponseDto, TodayDto, UpdateContentRequest,
        UserEventBatchRequest, UserEventIngestResponse,
    },
    datasource::{
        BbsDataSource, BbsFeedDataSource, BbsLinkDataSource, CommentDataSource, GrowthDataSource,
        LikeStatusDataSource, MediaDataSource, SearchMainDataSource, UpstreamError,
        UserEventDataSource,
    },
};

#[derive(Clone)]
pub(crate) struct GatewayService {
    growth: Arc<dyn GrowthDataSource>,
    bbs_feed: Arc<dyn BbsFeedDataSource>,
    bbs_link: Arc<dyn BbsLinkDataSource>,
    search_main: Arc<dyn SearchMainDataSource>,
    bbs: Arc<dyn BbsDataSource>,
    comment: Arc<dyn CommentDataSource>,
    like_status: Arc<dyn LikeStatusDataSource>,
    user_event: Arc<dyn UserEventDataSource>,
    media: Arc<dyn MediaDataSource>,
}

pub(crate) struct GatewayDependencies {
    pub(crate) growth: Arc<dyn GrowthDataSource>,
    pub(crate) bbs_feed: Arc<dyn BbsFeedDataSource>,
    pub(crate) bbs_link: Arc<dyn BbsLinkDataSource>,
    pub(crate) search_main: Arc<dyn SearchMainDataSource>,
    pub(crate) bbs: Arc<dyn BbsDataSource>,
    pub(crate) comment: Arc<dyn CommentDataSource>,
    pub(crate) like_status: Arc<dyn LikeStatusDataSource>,
    pub(crate) user_event: Arc<dyn UserEventDataSource>,
    pub(crate) media: Arc<dyn MediaDataSource>,
}

impl GatewayService {
    pub(crate) fn new(dependencies: GatewayDependencies) -> Self {
        Self {
            growth: dependencies.growth,
            bbs_feed: dependencies.bbs_feed,
            bbs_link: dependencies.bbs_link,
            search_main: dependencies.search_main,
            bbs: dependencies.bbs,
            comment: dependencies.comment,
            like_status: dependencies.like_status,
            user_event: dependencies.user_event,
            media: dependencies.media,
        }
    }

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
