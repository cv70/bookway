use std::collections::BTreeSet;

use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tonic::transport::Channel;
use uuid::Uuid;

use crate::{
    conf::Config,
    datasource::{CommunityNotificationJob, CommunityNotificationJobDao, UpstreamError},
};
use bookway_account_api::pb as account_pb;
use bookway_ad_center_api::pb as ad_center_pb;
use bookway_ad_main_api::pb as ad_main_pb;
use bookway_bbs_api::pb as bbs_pb;
use bookway_bbs_creator_api::pb as creator_pb;
use bookway_bbs_feed_api::pb as bbs_feed_pb;
use bookway_bbs_link_api::pb as bbs_link_pb;
use bookway_bbs_message_api::pb as message_pb;
use bookway_bbs_search_api::pb as search_pb;
use bookway_comment_api::pb as comment_pb;
use bookway_content_audit_api::pb as audit_pb;
use bookway_feedback_api::pb as feedback_pb;
use bookway_growth_api::pb as growth_pb;
use bookway_interaction_status_api::pb as like_pb;
use bookway_knowledge_catalog_api::pb as catalog_pb;
use bookway_mall_api::pb as mall_pb;
use bookway_mall_order_api::pb as mall_order_pb;
use bookway_media_api::pb as media_pb;
use bookway_recommend_main_api::pb as recommend_pb;
use bookway_search_main_api::pb as search_main_pb;
use bookway_user_event_api::pb as user_event_pb;

#[derive(Clone)]
pub(crate) struct Domain {
    pub(crate) config: Config,
    account: account_pb::account_client::AccountClient<Channel>,
    growth: growth_pb::growth_client::GrowthClient<Channel>,
    knowledge_catalog: catalog_pb::knowledge_catalog_client::KnowledgeCatalogClient<Channel>,
    bbs_feed: bbs_feed_pb::bbs_feed_client::BbsFeedClient<Channel>,
    bbs_link: bbs_link_pb::bbs_link_client::BbsLinkClient<Channel>,
    search_main: search_main_pb::search_main_client::SearchMainClient<Channel>,
    bbs: bbs_pb::bbs_client::BbsClient<Channel>,
    bbs_creator: creator_pb::bbs_creator_client::BbsCreatorClient<Channel>,
    bbs_message: message_pb::bbs_message_client::BbsMessageClient<Channel>,
    comment: comment_pb::comment_client::CommentClient<Channel>,
    interaction_status: like_pb::interaction_status_client::InteractionStatusClient<Channel>,
    user_event: user_event_pb::user_event_client::UserEventClient<Channel>,
    media: media_pb::media_client::MediaClient<Channel>,
    content_audit: audit_pb::content_audit_client::ContentAuditClient<Channel>,
    feedback: feedback_pb::feedback_client::FeedbackClient<Channel>,
    ad_center: ad_center_pb::ad_center_client::AdCenterClient<Channel>,
    ad_main: ad_main_pb::ad_main_client::AdMainClient<Channel>,
    mall: mall_pb::mall_client::MallClient<Channel>,
    mall_order: mall_order_pb::mall_order_client::MallOrderClient<Channel>,
    community_notifications: CommunityNotificationSink,
}

#[derive(Clone)]
enum CommunityNotificationSink {
    Postgres(CommunityNotificationJobDao),
    Direct,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DomainInitError {
    #[error("could not connect to an upstream service: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("could not initialize community notification storage: {0}")]
    Data(#[from] bookway_data::DataError),
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RouteJoinResult {
    pub(crate) journey: growth_pb::Journey,
    pub(crate) participation: bbs_pb::RouteParticipationState,
}

/// Search owns candidates; this read model carries optional creator facts
/// without giving either service ownership of the other's data.
#[derive(Clone, Debug)]
pub(crate) struct SearchDiscovery {
    pub(crate) response: search_pb::SearchResponse,
    pub(crate) creator_profiles: Vec<creator_pb::CreatorProfile>,
}

/// Verified by User Event against a served Feed or Search exposure before it
/// can enter ranking evaluation or model features.
#[derive(Clone, Debug)]
pub(crate) struct ContentAttribution {
    pub(crate) session_id: String,
    pub(crate) request_id: String,
    pub(crate) position: u32,
    pub(crate) source: user_event_pb::AttributionSource,
}

macro_rules! grpc_call {
    ($domain:expr, $client:ident, $service:literal, $method:ident, $request:expr) => {{
        let mut client = $domain.$client.clone();
        client
            .$method(service_request($service, $request)?)
            .await
            .map_err(|error| grpc_error($service, error))
            .map(tonic::Response::into_inner)
    }};
}

impl Domain {
    pub(crate) async fn new(config: Config) -> Result<Self, DomainInitError> {
        let community_notifications = match bookway_data::storage_mode()? {
            bookway_data::StorageMode::Postgres => CommunityNotificationSink::Postgres(
                CommunityNotificationJobDao::new(bookway_data::postgres_pool().await?),
            ),
            // Keep the local memory-mode workflow usable. Production deploys
            // PostgreSQL and therefore always uses the durable queue below.
            bookway_data::StorageMode::Memory => CommunityNotificationSink::Direct,
        };
        Ok(Self {
            account: account_pb::account_client::AccountClient::connect(config.account_url.clone())
                .await?,
            growth: growth_pb::growth_client::GrowthClient::connect(config.growth_url.clone())
                .await?,
            knowledge_catalog:
                catalog_pb::knowledge_catalog_client::KnowledgeCatalogClient::connect(
                    config.knowledge_catalog_url.clone(),
                )
                .await?,
            bbs_feed: bbs_feed_pb::bbs_feed_client::BbsFeedClient::connect(
                config.bbs_feed_url.clone(),
            )
            .await?,
            bbs_link: bbs_link_pb::bbs_link_client::BbsLinkClient::connect(
                config.bbs_link_url.clone(),
            )
            .await?,
            search_main: search_main_pb::search_main_client::SearchMainClient::connect(
                config.search_main_url.clone(),
            )
            .await?,
            bbs: bbs_pb::bbs_client::BbsClient::connect(config.bbs_url.clone()).await?,
            bbs_creator: creator_pb::bbs_creator_client::BbsCreatorClient::connect(
                config.bbs_creator_url.clone(),
            )
            .await?,
            bbs_message: message_pb::bbs_message_client::BbsMessageClient::connect(
                config.bbs_message_url.clone(),
            )
            .await?,
            comment: comment_pb::comment_client::CommentClient::connect(config.comment_url.clone())
                .await?,
            interaction_status:
                like_pb::interaction_status_client::InteractionStatusClient::connect(
                    config.interaction_status_url.clone(),
                )
                .await?,
            user_event: user_event_pb::user_event_client::UserEventClient::connect(
                config.user_event_url.clone(),
            )
            .await?,
            media: media_pb::media_client::MediaClient::connect(config.media_url.clone()).await?,
            content_audit: audit_pb::content_audit_client::ContentAuditClient::connect(
                config.content_audit_url.clone(),
            )
            .await?,
            feedback: feedback_pb::feedback_client::FeedbackClient::connect(
                config.feedback_url.clone(),
            )
            .await?,
            ad_center: ad_center_pb::ad_center_client::AdCenterClient::connect(
                config.ad_center_url.clone(),
            )
            .await?,
            ad_main: ad_main_pb::ad_main_client::AdMainClient::connect(config.ad_main_url.clone())
                .await?,
            mall: mall_pb::mall_client::MallClient::connect(config.mall_url.clone()).await?,
            mall_order: mall_order_pb::mall_order_client::MallOrderClient::connect(
                config.mall_order_url.clone(),
            )
            .await?,
            community_notifications,
            config,
        })
    }

    pub(crate) async fn account_profile(
        &self,
        request: account_pb::ProfileRequest,
    ) -> Result<account_pb::AccountProfile, UpstreamError> {
        grpc_call!(self, account, "account", profile, request)
    }

    pub(crate) async fn creator_profile(
        &self,
        request: creator_pb::CreatorProfileRequest,
    ) -> Result<creator_pb::CreatorProfile, UpstreamError> {
        grpc_call!(self, bbs_creator, "bbs-creator", get_profile, request)
    }

    pub(crate) async fn public_creator_profile(
        &self,
        viewer_id: &str,
        request: creator_pb::CreatorProfileRequest,
    ) -> Result<creator_pb::CreatorProfile, UpstreamError> {
        if self
            .visibility(viewer_id)
            .await?
            .iter()
            .any(|excluded_user_id| excluded_user_id == &request.user_id)
        {
            return Err(hidden_content());
        }
        self.creator_profile(request).await
    }

    pub(crate) async fn update_creator_profile(
        &self,
        request: creator_pb::UpsertCreatorProfileRequest,
    ) -> Result<creator_pb::CreatorProfile, UpstreamError> {
        grpc_call!(self, bbs_creator, "bbs-creator", upsert_profile, request)
    }

    pub(crate) async fn creator_profiles(
        &self,
        request: creator_pb::ListCreatorProfilesRequest,
    ) -> Result<creator_pb::CreatorProfilePage, UpstreamError> {
        grpc_call!(self, bbs_creator, "bbs-creator", list_profiles, request)
    }

    pub(crate) async fn public_creator_profiles(
        &self,
        viewer_id: &str,
        mut request: creator_pb::ListCreatorProfilesRequest,
    ) -> Result<creator_pb::CreatorProfilePage, UpstreamError> {
        request.excluded_user_ids = self.visibility(viewer_id).await?;
        self.creator_profiles(request).await
    }

    pub(crate) async fn send_direct_message(
        &self,
        request: message_pb::SendDirectMessageRequest,
    ) -> Result<message_pb::DirectMessage, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", send, request)
    }

    pub(crate) async fn direct_conversations(
        &self,
        request: message_pb::ListConversationsRequest,
    ) -> Result<message_pb::ConversationPage, UpstreamError> {
        grpc_call!(
            self,
            bbs_message,
            "bbs-message",
            list_conversations,
            request
        )
    }

    pub(crate) async fn direct_messages(
        &self,
        request: message_pb::ListMessagesRequest,
    ) -> Result<message_pb::DirectMessagePage, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", list_messages, request)
    }

    pub(crate) async fn mark_direct_conversation_read(
        &self,
        request: message_pb::MarkConversationReadRequest,
    ) -> Result<message_pb::MarkConversationReadResponse, UpstreamError> {
        grpc_call!(
            self,
            bbs_message,
            "bbs-message",
            mark_conversation_read,
            request
        )
    }

    pub(crate) async fn direct_message_preferences(
        &self,
        request: message_pb::UserRequest,
    ) -> Result<message_pb::DirectMessagePreferences, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", get_preferences, request)
    }

    pub(crate) async fn update_direct_message_preferences(
        &self,
        request: message_pb::UpdateDirectMessagePreferencesRequest,
    ) -> Result<message_pb::DirectMessagePreferences, UpstreamError> {
        grpc_call!(
            self,
            bbs_message,
            "bbs-message",
            update_preferences,
            request
        )
    }

    pub(crate) async fn report_direct_message(
        &self,
        request: message_pb::ReportDirectMessageRequest,
    ) -> Result<message_pb::DirectMessageReport, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", report, request)
    }

    pub(crate) async fn moderation_direct_message_reports(
        &self,
        request: message_pb::ListDirectMessageReportsRequest,
    ) -> Result<message_pb::DirectMessageReportPage, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", list_reports, request)
    }

    pub(crate) async fn review_direct_message_report(
        &self,
        request: message_pb::ReviewDirectMessageReportRequest,
    ) -> Result<message_pb::DirectMessageReport, UpstreamError> {
        grpc_call!(self, bbs_message, "bbs-message", review_report, request)
    }

    pub(crate) async fn update_account_profile(
        &self,
        request: account_pb::UpdateProfileRequest,
    ) -> Result<account_pb::AccountProfile, UpstreamError> {
        grpc_call!(self, account, "account", update_profile, request)
    }

    pub(crate) async fn create_feedback(
        &self,
        request: feedback_pb::CreateFeedbackRequest,
    ) -> Result<feedback_pb::FeedbackItem, UpstreamError> {
        grpc_call!(self, feedback, "feedback", create_feedback, request)
    }

    pub(crate) async fn own_feedback(
        &self,
        request: feedback_pb::ListOwnFeedbackRequest,
    ) -> Result<feedback_pb::FeedbackList, UpstreamError> {
        grpc_call!(self, feedback, "feedback", list_own_feedback, request)
    }

    pub(crate) async fn moderation_feedback(
        &self,
        request: feedback_pb::ListFeedbackRequest,
    ) -> Result<feedback_pb::FeedbackList, UpstreamError> {
        grpc_call!(self, feedback, "feedback", list_feedback, request)
    }

    pub(crate) async fn review_feedback(
        &self,
        request: feedback_pb::ReviewFeedbackRequest,
    ) -> Result<feedback_pb::FeedbackItem, UpstreamError> {
        grpc_call!(self, feedback, "feedback", review_feedback, request)
    }

    pub(crate) async fn list_journeys(
        &self,
        request: growth_pb::UserRequest,
    ) -> Result<growth_pb::JourneyList, UpstreamError> {
        grpc_call!(self, growth, "growth", list_journeys, request)
    }

    pub(crate) async fn create_journey(
        &self,
        request: growth_pb::CreateJourneyRequest,
    ) -> Result<growth_pb::Journey, UpstreamError> {
        grpc_call!(self, growth, "growth", create_journey, request)
    }

    pub(crate) async fn get_journey(
        &self,
        request: growth_pb::JourneyRequest,
    ) -> Result<growth_pb::JourneyDetail, UpstreamError> {
        grpc_call!(self, growth, "growth", get_journey, request)
    }

    pub(crate) async fn update_journey(
        &self,
        request: growth_pb::UpdateJourneyRequest,
    ) -> Result<growth_pb::Journey, UpstreamError> {
        grpc_call!(self, growth, "growth", update_journey, request)
    }

    pub(crate) async fn create_action(
        &self,
        request: growth_pb::CreateActionRequest,
    ) -> Result<growth_pb::Action, UpstreamError> {
        grpc_call!(self, growth, "growth", create_action, request)
    }

    pub(crate) async fn today(
        &self,
        request: growth_pb::ScheduleRequest,
    ) -> Result<growth_pb::TodaySummary, UpstreamError> {
        grpc_call!(self, growth, "growth", today, request)
    }

    pub(crate) async fn complete_action(
        &self,
        request: growth_pb::CompleteActionRequest,
    ) -> Result<growth_pb::Action, UpstreamError> {
        let user_id = request.user_id.clone();
        let completion = grpc_call!(self, growth, "growth", complete_action, request)?;
        let action = completion.action.ok_or_else(|| UpstreamError::Grpc {
            service: "growth",
            code: tonic::Code::Internal,
            message: "Growth returned an action completion without its action".to_string(),
        })?;
        if let Some(source_route_id) = completion
            .source_route_id
            .as_deref()
            .filter(|route_id| !route_id.trim().is_empty())
            && let Err(error) = self
                .record_route_action_completion(&user_id, source_route_id, &action.id)
                .await
        {
            // An executed action must stay successful if analytics is temporarily unavailable.
            tracing::warn!(%error, user_id, source_route_id, action_id = %action.id, "route completion attribution degraded");
        } else if let Some(source_content_id) = completion
            .source_knowledge_content_id
            .as_deref()
            .filter(|content_id| !content_id.trim().is_empty())
            && let Err(error) = self
                .record_knowledge_action_completion(&user_id, source_content_id, &action.id)
                .await
        {
            // The private action is committed already; analytics must remain a
            // best-effort enrichment rather than make completion ambiguous.
            tracing::warn!(%error, user_id, source_content_id, action_id = %action.id, "knowledge completion attribution degraded");
        }
        Ok(action)
    }

    pub(crate) async fn update_action(
        &self,
        request: growth_pb::UpdateActionRequest,
    ) -> Result<growth_pb::Action, UpstreamError> {
        grpc_call!(self, growth, "growth", update_action, request)
    }

    pub(crate) async fn reminder_preferences(
        &self,
        request: growth_pb::UserRequest,
    ) -> Result<growth_pb::ReminderPreference, UpstreamError> {
        grpc_call!(self, growth, "growth", reminder_preferences, request)
    }

    pub(crate) async fn update_reminder_preferences(
        &self,
        request: growth_pb::UpdateReminderPreferencesRequest,
    ) -> Result<growth_pb::ReminderPreference, UpstreamError> {
        grpc_call!(self, growth, "growth", update_reminder_preferences, request)
    }

    pub(crate) async fn register_push_device(
        &self,
        request: growth_pb::RegisterPushDeviceRequest,
    ) -> Result<growth_pb::PushDevice, UpstreamError> {
        grpc_call!(self, growth, "growth", register_push_device, request)
    }

    pub(crate) async fn revoke_push_device(
        &self,
        request: growth_pb::PushDeviceRequest,
    ) -> Result<(), UpstreamError> {
        grpc_call!(self, growth, "growth", revoke_push_device, request).map(|_| ())
    }

    pub(crate) async fn list_notifications(
        &self,
        request: growth_pb::NotificationQueryRequest,
    ) -> Result<growth_pb::NotificationPage, UpstreamError> {
        grpc_call!(self, growth, "growth", list_notifications, request)
    }

    pub(crate) async fn mark_notification_read(
        &self,
        request: growth_pb::NotificationRequest,
    ) -> Result<growth_pb::UserNotification, UpstreamError> {
        grpc_call!(self, growth, "growth", mark_notification_read, request)
    }

    pub(crate) async fn list_entries(
        &self,
        request: growth_pb::UserRequest,
    ) -> Result<growth_pb::EntryList, UpstreamError> {
        grpc_call!(self, growth, "growth", list_entries, request)
    }

    pub(crate) async fn create_entry(
        &self,
        request: growth_pb::CreateEntryRequest,
    ) -> Result<growth_pb::GrowthEntry, UpstreamError> {
        grpc_call!(self, growth, "growth", create_entry, request)
    }

    pub(crate) async fn retry_entry_publication(
        &self,
        request: growth_pb::RetryEntryPublicationRequest,
    ) -> Result<growth_pb::GrowthEntry, UpstreamError> {
        grpc_call!(self, growth, "growth", retry_entry_publication, request)
    }

    pub(crate) async fn weekly_review(
        &self,
        request: growth_pb::UserRequest,
    ) -> Result<growth_pb::WeeklyReviewSummary, UpstreamError> {
        grpc_call!(self, growth, "growth", weekly_review, request)
    }

    pub(crate) async fn save_weekly_review(
        &self,
        request: growth_pb::SaveWeeklyReviewRequest,
    ) -> Result<growth_pb::ReviewRecord, UpstreamError> {
        grpc_call!(self, growth, "growth", save_weekly_review, request)
    }

    pub(crate) async fn apply_weekly_review_adjustment(
        &self,
        request: growth_pb::ApplyWeeklyReviewAdjustmentRequest,
    ) -> Result<growth_pb::ApplyWeeklyReviewAdjustmentResponse, UpstreamError> {
        grpc_call!(
            self,
            growth,
            "growth",
            apply_weekly_review_adjustment,
            request
        )
    }

    pub(crate) async fn companion(
        &self,
        request: growth_pb::ScheduleRequest,
    ) -> Result<growth_pb::CompanionBrief, UpstreamError> {
        grpc_call!(self, growth, "growth", companion, request)
    }

    pub(crate) async fn list_knowledge(
        &self,
        request: growth_pb::KnowledgeQueryRequest,
    ) -> Result<growth_pb::KnowledgeList, UpstreamError> {
        grpc_call!(self, growth, "growth", list_knowledge, request)
    }

    pub(crate) async fn create_knowledge(
        &self,
        request: growth_pb::CreateKnowledgeRequest,
    ) -> Result<growth_pb::KnowledgeResource, UpstreamError> {
        grpc_call!(self, growth, "growth", create_knowledge, request)
    }

    pub(crate) async fn start_knowledge_journey(
        &self,
        request: growth_pb::StartKnowledgeJourneyRequest,
    ) -> Result<growth_pb::KnowledgeJourney, UpstreamError> {
        grpc_call!(self, growth, "growth", start_knowledge_journey, request)
    }

    pub(crate) async fn update_knowledge(
        &self,
        request: growth_pb::UpdateKnowledgeRequest,
    ) -> Result<growth_pb::KnowledgeResource, UpstreamError> {
        grpc_call!(self, growth, "growth", update_knowledge, request)
    }

    pub(crate) async fn feed(
        &self,
        request: recommend_pb::FeedRequest,
    ) -> Result<recommend_pb::FeedResponse, UpstreamError> {
        grpc_call!(self, bbs_feed, "bbs-feed", feed, request)
    }

    pub(crate) async fn get_content(
        &self,
        user_id: &str,
        content_id: &str,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        self.public_content_for_viewer(user_id, content_id).await
    }

    pub(crate) async fn create_content(
        &self,
        request: bbs_link_pb::CreateRequest,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        grpc_call!(self, bbs_link, "bbs-link", create, request)
    }

    pub(crate) async fn fork_route(
        &self,
        request: bbs_link_pb::ForkRouteRequest,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        grpc_call!(self, bbs_link, "bbs-link", fork_route, request)
    }

    pub(crate) async fn update_content(
        &self,
        request: bbs_link_pb::UpdateRequest,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        grpc_call!(self, bbs_link, "bbs-link", update, request)
    }

    pub(crate) async fn publish_content(
        &self,
        request: bbs_link_pb::PublishRequest,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        grpc_call!(self, bbs_link, "bbs-link", publish, request)
    }

    pub(crate) async fn search_resources(
        &self,
        request: catalog_pb::SearchRequest,
    ) -> Result<catalog_pb::SearchResponse, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            search,
            request
        )
    }

    pub(crate) async fn get_resource(
        &self,
        resource_id: String,
    ) -> Result<catalog_pb::Resource, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            get,
            catalog_pb::GetRequest { resource_id }
        )
    }

    pub(crate) async fn list_route_node_resources(
        &self,
        request: catalog_pb::ListNodeResourcesRequest,
    ) -> Result<catalog_pb::ListNodeResourcesResponse, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            list_node_resources,
            request
        )
    }

    pub(crate) async fn attach_route_node_resource(
        &self,
        request: catalog_pb::AttachNodeResourceRequest,
    ) -> Result<catalog_pb::RouteNodeResourceAttachment, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            attach_node_resource,
            request
        )
    }

    pub(crate) async fn detach_route_node_resource(
        &self,
        request: catalog_pb::DetachNodeResourceRequest,
    ) -> Result<catalog_pb::DetachNodeResourceResponse, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            detach_node_resource,
            request
        )
    }

    pub(crate) async fn retrieve_route_node_rag_context(
        &self,
        request: catalog_pb::RetrieveRagContextRequest,
    ) -> Result<catalog_pb::RetrieveRagContextResponse, UpstreamError> {
        grpc_call!(
            self,
            knowledge_catalog,
            "knowledge-catalog",
            retrieve_rag_context,
            request
        )
    }

    pub(crate) async fn capture_resource_as_knowledge(
        &self,
        user_id: String,
        resource_id: String,
    ) -> Result<growth_pb::KnowledgeResource, UpstreamError> {
        let resource = self.get_resource(resource_id.clone()).await?;
        let request = public_resource_knowledge_request(&user_id, &resource)?;
        self.create_knowledge(request).await
    }

    pub(crate) async fn accept_question_answer(
        &self,
        user_id: String,
        question_id: String,
        answer_id: String,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        let question = self
            .public_content_for_viewer(&user_id, &question_id)
            .await?;
        if question.content_type != bbs_link_pb::ContentType::Question as i32 {
            return Err(route_precondition("只有问题内容可以采纳回答"));
        }
        if question.author_id != user_id {
            return Err(permission_denied("只有问题作者可以采纳回答"));
        }
        let answer = grpc_call!(
            self,
            comment,
            "comment",
            get,
            comment_pb::GetRequest {
                post_id: question.id.clone(),
                comment_id: answer_id.clone(),
                excluded_author_ids: self.visibility(&user_id).await?,
            }
        )?;
        if answer.post_id != question.id
            || answer.parent_id.is_some()
            || answer.status != comment_pb::CommentStatus::Published as i32
        {
            return Err(upstream_invalid("只能采纳该问题下已公开的一级回答"));
        }
        let accepted = grpc_call!(
            self,
            bbs_link,
            "bbs-link",
            accept_answer,
            bbs_link_pb::AcceptAnswerRequest {
                user_id: user_id.clone(),
                question_id: question.id.clone(),
                answer_id: answer.id.clone(),
            }
        )?;
        if answer.author_id != user_id {
            self.create_community_notification(
                &answer.author_id,
                notification(
                    format!("question-answer-accepted:{}", answer.id),
                    "你的回答被采纳了",
                    "问题作者已将你的回答设为最佳答案",
                    [
                        ("post_id", question.id.as_str()),
                        ("comment_id", answer.id.as_str()),
                        ("interaction", "answer_accepted"),
                    ],
                ),
            )
            .await;
        }
        Ok(accepted)
    }

    pub(crate) async fn search(
        &self,
        mut request: search_pb::SearchRequest,
    ) -> Result<SearchDiscovery, UpstreamError> {
        let user_id = request
            .user_id
            .clone()
            .unwrap_or_else(|| "anonymous".to_string());
        let excluded_author_ids = self.visibility(&user_id).await?;
        request.excluded_author_ids = excluded_author_ids.clone();
        let mut response = grpc_call!(self, search_main, "search-main", search, request)?;
        let creator_profiles = self
            .enrich_search_creators(&mut response, &excluded_author_ids)
            .await;
        let route_ids = response
            .items
            .iter()
            .filter_map(|item| item.post.as_ref())
            .filter(|post| post.is_route)
            .map(|post| post.id.clone())
            .collect::<Vec<_>>();
        if !route_ids.is_empty() {
            let context = grpc_call!(
                self,
                bbs,
                "bbs",
                route_context,
                bbs_pb::RouteContextRequest { user_id, route_ids }
            );
            match context {
                Ok(context) => {
                    for post in response
                        .items
                        .iter_mut()
                        .filter_map(|item| item.post.as_mut())
                    {
                        post.join_count = post.join_count.saturating_add(
                            u32::try_from(
                                context
                                    .participant_counts
                                    .get(&post.id)
                                    .copied()
                                    .unwrap_or_default(),
                            )
                            .unwrap_or(u32::MAX),
                        );
                    }
                }
                Err(error) => {
                    response.degraded = true;
                    tracing::warn!(%error, "search route context degraded");
                }
            }
        }
        Ok(SearchDiscovery {
            response,
            creator_profiles,
        })
    }

    pub(crate) async fn suggestions(
        &self,
        user_id: String,
        query: String,
    ) -> Result<search_pb::SuggestionsResponse, UpstreamError> {
        grpc_call!(
            self,
            search_main,
            "search-main",
            suggestions,
            search_pb::SuggestionsRequest {
                q: query,
                user_id: Some(user_id.clone()),
                excluded_author_ids: self.visibility(&user_id).await?,
            }
        )
    }

    pub(crate) async fn ingest_events(
        &self,
        request: user_event_pb::IngestRequest,
    ) -> Result<user_event_pb::IngestResponse, UpstreamError> {
        grpc_call!(self, user_event, "user-event", ingest, request)
    }

    pub(crate) async fn create_media_upload(
        &self,
        request: media_pb::CreateUploadRequest,
    ) -> Result<media_pb::UploadResponse, UpstreamError> {
        grpc_call!(self, media, "media", create_upload, request)
    }

    pub(crate) async fn complete_media_upload(
        &self,
        request: media_pb::ResourceRequest,
    ) -> Result<media_pb::MediaResource, UpstreamError> {
        grpc_call!(self, media, "media", complete_upload, request)
    }

    pub(crate) async fn get_media(
        &self,
        request: media_pb::ResourceRequest,
    ) -> Result<media_pb::MediaResource, UpstreamError> {
        grpc_call!(self, media, "media", get, request)
    }

    pub(crate) async fn set_reaction(
        &self,
        request: like_pb::SetReactionRequest,
        negative_feedback_reason: Option<user_event_pb::NegativeFeedbackReason>,
        attribution: Option<ContentAttribution>,
    ) -> Result<like_pb::Reaction, UpstreamError> {
        let content = self
            .public_content_for_viewer(&request.user_id, &request.post_id)
            .await?;
        let actor_id = request.user_id.clone();
        let post_id = request.post_id.clone();
        let reaction = grpc_call!(
            self,
            interaction_status,
            "interaction-status",
            set_reaction,
            request
        )?;
        if reaction.active
            && let Ok(reaction_type) = like_pb::ReactionType::try_from(reaction.reaction)
            && let Err(error) = self
                .record_reaction_signal(
                    &actor_id,
                    &post_id,
                    reaction_type,
                    negative_feedback_reason,
                    attribution.as_ref(),
                )
                .await
        {
            // The interaction state is already committed. The stable event ID
            // makes a later retry safe, so analytics degradation must not turn
            // a successful reaction into an ambiguous client failure.
            tracing::warn!(%error, user_id = actor_id, post_id, "reaction recommendation signal degraded");
        }
        if reaction.reaction == like_pb::ReactionType::Like as i32
            && reaction.active
            && content.author_id != actor_id
        {
            self.create_community_notification(
                &content.author_id,
                notification(
                    format!("like:{actor_id}:{post_id}"),
                    "收到一个赞",
                    "有人赞了你的内容",
                    [
                        ("post_id", post_id.as_str()),
                        ("actor_id", actor_id.as_str()),
                        ("interaction", "like"),
                    ],
                ),
            )
            .await;
        }
        Ok(reaction)
    }

    pub(crate) async fn social_context(
        &self,
        user_id: String,
    ) -> Result<bbs_pb::SocialContext, UpstreamError> {
        grpc_call!(
            self,
            bbs,
            "bbs",
            context,
            bbs_pb::ContextRequest {
                user_id,
                post_ids: Vec::new(),
            }
        )
    }

    pub(crate) async fn list_route_participations(
        &self,
        user_id: String,
    ) -> Result<bbs_pb::RouteParticipationList, UpstreamError> {
        grpc_call!(
            self,
            bbs,
            "bbs",
            list_route_participations,
            bbs_pb::ContextRequest {
                user_id,
                post_ids: Vec::new(),
            }
        )
    }

    pub(crate) async fn set_route_participation(
        &self,
        request: bbs_pb::RouteParticipationRequest,
    ) -> Result<bbs_pb::RouteParticipationState, UpstreamError> {
        self.set_route_participation_with_attribution(request, None)
            .await
    }

    async fn set_route_participation_with_attribution(
        &self,
        mut request: bbs_pb::RouteParticipationRequest,
        attribution: Option<&ContentAttribution>,
    ) -> Result<bbs_pb::RouteParticipationState, UpstreamError> {
        let user_id = request.user_id.clone();
        let route_id = request.route_id.clone();
        let active = request.active;
        if request.active {
            if let Some(journey_id) = request.private_journey_id.clone() {
                self.get_journey(growth_pb::JourneyRequest {
                    user_id: request.user_id.clone(),
                    journey_id,
                })
                .await?;
            }
        } else {
            request.private_journey_id = None;
        }
        let intent = grpc_call!(
            self,
            growth,
            "growth",
            set_route_participation_intent,
            growth_pb::SetRouteParticipationIntentRequest {
                user_id: request.user_id.clone(),
                route_id: request.route_id.clone(),
                active: request.active,
                private_journey_id: request.private_journey_id.clone(),
            }
        )?;
        request.intent_version = Some(intent.version);
        let participation = grpc_call!(self, bbs, "bbs", set_route_participation, request)?;
        if active
            && participation.joined
            && let Err(error) = self
                .record_route_join(&user_id, &route_id, attribution)
                .await
        {
            // The adoption fact is committed by Growth and BBS already. A
            // recommendation signal outage must not make the user-facing join
            // ambiguous; the stable event ID collapses a later retry.
            tracing::warn!(%error, user_id, route_id, "route join attribution degraded");
        }
        Ok(participation)
    }

    pub(crate) async fn join_route(
        &self,
        user_id: String,
        route_id: String,
        attribution: Option<ContentAttribution>,
    ) -> Result<RouteJoinResult, UpstreamError> {
        let content = self.public_content_for_viewer(&user_id, &route_id).await?;
        let route_request = route_journey_request(&user_id, &route_id, &content)?;
        let journey = grpc_call!(self, growth, "growth", create_route_journey, route_request)?;
        let participation = self
            .set_route_participation_with_attribution(
                bbs_pb::RouteParticipationRequest {
                    user_id: user_id.clone(),
                    route_id: route_id.clone(),
                    active: true,
                    private_journey_id: Some(journey.id.clone()),
                    intent_version: None,
                },
                attribution.as_ref(),
            )
            .await?;
        Ok(RouteJoinResult {
            journey,
            participation,
        })
    }

    /// Captures a public item as a private reference. We intentionally retain
    /// no body or media: future opens must re-check content visibility and
    /// moderation state through BBS Link.
    pub(crate) async fn capture_content_as_knowledge(
        &self,
        user_id: String,
        content_id: String,
        attribution: Option<ContentAttribution>,
    ) -> Result<growth_pb::KnowledgeResource, UpstreamError> {
        let content = self
            .public_content_for_viewer(&user_id, &content_id)
            .await?;
        let resource = grpc_call!(
            self,
            growth,
            "growth",
            create_knowledge,
            content_knowledge_request(&content)?
        )?;
        if let Err(error) = self.mark_content_bookmarked(&user_id, &content.id).await {
            // The captured resource is the user-visible source of truth. A
            // retry will converge the lightweight reaction state later.
            tracing::warn!(%error, user_id, content_id = %content.id, "knowledge capture bookmark state degraded");
        }
        if let Err(error) = self
            .record_content_knowledge_capture(&user_id, &content.id, attribution.as_ref())
            .await
        {
            // Recommendation feedback must not make a successful capture
            // ambiguous. The event ID is stable if the user retries.
            tracing::warn!(%error, user_id, content_id = %content.id, "knowledge capture attribution degraded");
        }
        Ok(resource)
    }

    pub(crate) async fn report_content(
        &self,
        request: audit_pb::CreateReportRequest,
    ) -> Result<audit_pb::ContentReport, UpstreamError> {
        self.public_content_for_viewer(&request.reporter_id, &request.content_id)
            .await?;
        grpc_call!(self, content_audit, "content-audit", report, request)
    }

    pub(crate) async fn moderation_reports(
        &self,
        request: audit_pb::ListReportsRequest,
    ) -> Result<audit_pb::ReportPage, UpstreamError> {
        grpc_call!(self, content_audit, "content-audit", list_reports, request)
    }

    pub(crate) async fn review_report(
        &self,
        request: audit_pb::ReviewReportRequest,
    ) -> Result<audit_pb::ContentReport, UpstreamError> {
        let report = grpc_call!(self, content_audit, "content-audit", review_report, request)?;
        if report.action == audit_pb::ContentAction::Restrict as i32 {
            let _ = grpc_call!(
                self,
                bbs_link,
                "bbs-link",
                restrict,
                bbs_link_pb::RestrictRequest {
                    content_id: report.content_id.clone(),
                }
            );
        }
        Ok(report)
    }

    pub(crate) async fn appeal_content(
        &self,
        request: audit_pb::CreateAppealRequest,
    ) -> Result<audit_pb::ContentAppeal, UpstreamError> {
        let content = grpc_call!(
            self,
            bbs_link,
            "bbs-link",
            get,
            bbs_link_pb::IdRequest {
                id: request.content_id.clone(),
            }
        )?;
        if content.author_id != request.appellant_id {
            return Err(permission_denied("only the content author can appeal"));
        }
        if content.status != bbs_link_pb::ContentStatus::Restricted as i32 {
            return Err(route_precondition(
                "only restricted content can be appealed",
            ));
        }
        grpc_call!(self, content_audit, "content-audit", appeal, request)
    }

    pub(crate) async fn own_contents(
        &self,
        mut request: bbs_link_pb::ListRequest,
        user_id: String,
    ) -> Result<bbs_link_pb::ContentPage, UpstreamError> {
        request.author_id = Some(user_id);
        request.ids = None;
        grpc_call!(self, bbs_link, "bbs-link", list, request)
    }

    pub(crate) async fn public_author_contents(
        &self,
        viewer_id: &str,
        author_id: &str,
        cursor: Option<String>,
        limit: Option<u32>,
    ) -> Result<bbs_link_pb::ContentPage, UpstreamError> {
        let author_id = author_id.trim();
        if author_id.is_empty() {
            return Err(upstream_invalid("author id is required"));
        }
        if self
            .visibility(viewer_id)
            .await?
            .iter()
            .any(|excluded_author_id| excluded_author_id == author_id)
        {
            return Err(hidden_content());
        }
        grpc_call!(
            self,
            bbs_link,
            "bbs-link",
            list,
            public_author_content_request(author_id, cursor, limit)
        )
    }

    pub(crate) async fn own_appeals(
        &self,
        mut request: audit_pb::ListAppealsRequest,
        user_id: String,
    ) -> Result<audit_pb::AppealPage, UpstreamError> {
        request.appellant_id = Some(user_id);
        request.content_id = None;
        grpc_call!(self, content_audit, "content-audit", list_appeals, request)
    }

    pub(crate) async fn moderation_appeals(
        &self,
        mut request: audit_pb::ListAppealsRequest,
    ) -> Result<audit_pb::AppealPage, UpstreamError> {
        request
            .status
            .get_or_insert(audit_pb::AppealStatus::Pending as i32);
        grpc_call!(self, content_audit, "content-audit", list_appeals, request)
    }

    pub(crate) async fn review_appeal(
        &self,
        request: audit_pb::ReviewAppealRequest,
    ) -> Result<audit_pb::ContentAppeal, UpstreamError> {
        let appeal = grpc_call!(self, content_audit, "content-audit", review_appeal, request)?;
        if appeal.action == audit_pb::ContentAction::Restore as i32 {
            let _ = grpc_call!(
                self,
                bbs_link,
                "bbs-link",
                restore,
                bbs_link_pb::RestoreRequest {
                    content_id: appeal.content_id.clone(),
                }
            );
        }
        Ok(appeal)
    }

    pub(crate) async fn comments(
        &self,
        user_id: String,
        mut request: comment_pb::ListRequest,
    ) -> Result<comment_pb::CommentPage, UpstreamError> {
        request.excluded_author_ids = self.visibility(&user_id).await?;
        self.public_content_for_viewer(&user_id, &request.post_id)
            .await?;
        grpc_call!(self, comment, "comment", list, request)
    }

    pub(crate) async fn create_comment(
        &self,
        mut request: comment_pb::CreateRequest,
    ) -> Result<comment_pb::CommentItem, UpstreamError> {
        let content = self
            .public_content_for_viewer(&request.user_id, &request.post_id)
            .await?;
        request.excluded_author_ids = self.visibility(&request.user_id).await?;
        let actor_id = request.user_id.clone();
        let post_id = request.post_id.clone();
        let result = grpc_call!(self, comment, "comment", create, request)?;
        let comment = result
            .comment
            .as_ref()
            .ok_or_else(|| upstream_invalid("comment service returned no comment"))?;
        for (recipient, event) in comment_notifications(
            &content.author_id,
            comment,
            result.parent_author_id.as_deref(),
            &actor_id,
            &post_id,
        ) {
            self.create_community_notification(&recipient, event).await;
        }
        Ok(comment.clone())
    }

    pub(crate) async fn moderation_comments(
        &self,
        request: comment_pb::ListModerationRequest,
    ) -> Result<comment_pb::ModerationCommentPage, UpstreamError> {
        grpc_call!(self, comment, "comment", list_moderation, request)
    }

    pub(crate) async fn review_moderation_comment(
        &self,
        request: comment_pb::ReviewCommentRequest,
    ) -> Result<comment_pb::CommentItem, UpstreamError> {
        let result = grpc_call!(self, comment, "comment", review, request)?;
        let comment = result
            .comment
            .as_ref()
            .ok_or_else(|| upstream_invalid("comment service returned no comment"))?;
        self.create_community_notification(
            &comment.author_id,
            comment_moderation_notification(comment),
        )
        .await;
        if comment.status == comment_pb::CommentStatus::Published as i32 {
            let actor_id = comment.author_id.clone();
            match grpc_call!(
                self,
                bbs_link,
                "bbs-link",
                get_public,
                bbs_link_pb::IdRequest {
                    id: comment.post_id.clone(),
                }
            ) {
                Ok(content) => {
                    for (recipient, event) in comment_notifications(
                        &content.author_id,
                        comment,
                        result.parent_author_id.as_deref(),
                        &actor_id,
                        &comment.post_id,
                    ) {
                        self.create_community_notification(&recipient, event).await;
                    }
                }
                Err(error) => {
                    // The human decision is already committed. A later review retry can
                    // safely replay this notification once the post is public again.
                    tracing::warn!(%error, comment_id = %comment.id, "comment approval notification deferred");
                }
            }
        }
        Ok(comment.clone())
    }

    pub(crate) async fn delete_comment(
        &self,
        request: comment_pb::DeleteRequest,
    ) -> Result<(), UpstreamError> {
        grpc_call!(self, comment, "comment", delete, request).map(|_| ())
    }

    pub(crate) async fn report_comment(
        &self,
        viewer_id: String,
        mut request: comment_pb::CreateCommentReportRequest,
    ) -> Result<comment_pb::CommentReport, UpstreamError> {
        self.public_content_for_viewer(&viewer_id, &request.post_id)
            .await?;
        request.reporter_id = viewer_id.clone();
        request.excluded_author_ids = self.visibility(&viewer_id).await?;
        grpc_call!(self, comment, "comment", report, request)
    }

    pub(crate) async fn own_comment_appeals(
        &self,
        mut request: comment_pb::ListCommentAppealsRequest,
        user_id: String,
    ) -> Result<comment_pb::CommentAppealPage, UpstreamError> {
        request.author_id = Some(user_id);
        grpc_call!(self, comment, "comment", list_appeals, request)
    }

    pub(crate) async fn moderation_comment_reports(
        &self,
        request: comment_pb::ListCommentReportsRequest,
    ) -> Result<comment_pb::CommentReportPage, UpstreamError> {
        grpc_call!(self, comment, "comment", list_reports, request)
    }

    pub(crate) async fn review_comment_report(
        &self,
        request: comment_pb::ReviewCommentReportRequest,
    ) -> Result<comment_pb::CommentReport, UpstreamError> {
        let report = grpc_call!(self, comment, "comment", review_report, request)?;
        if report.status == comment_pb::CommentReportStatus::Resolved as i32
            && report.action == comment_pb::CommentReportAction::RestrictComment as i32
            && let Some(comment) = report.reported_comment.as_ref()
        {
            self.create_community_notification(
                &comment.author_id,
                comment_moderation_notification(comment),
            )
            .await;
        }
        Ok(report)
    }

    pub(crate) async fn appeal_comment(
        &self,
        author_id: String,
        mut request: comment_pb::CreateCommentAppealRequest,
    ) -> Result<comment_pb::CommentAppeal, UpstreamError> {
        request.author_id = author_id;
        grpc_call!(self, comment, "comment", appeal, request)
    }

    pub(crate) async fn moderation_comment_appeals(
        &self,
        request: comment_pb::ListCommentAppealsRequest,
    ) -> Result<comment_pb::CommentAppealPage, UpstreamError> {
        grpc_call!(self, comment, "comment", list_appeals, request)
    }

    pub(crate) async fn review_comment_appeal(
        &self,
        request: comment_pb::ReviewCommentAppealRequest,
    ) -> Result<comment_pb::CommentAppeal, UpstreamError> {
        let appeal = grpc_call!(self, comment, "comment", review_appeal, request)?;
        if appeal.status == comment_pb::CommentAppealStatus::Resolved as i32
            && appeal.action == comment_pb::CommentAppealAction::RestoreComment as i32
            && let Some(comment) = appeal.appealed_comment.as_ref()
        {
            self.create_community_notification(
                &comment.author_id,
                comment_moderation_notification(comment),
            )
            .await;
        }
        Ok(appeal)
    }

    pub(crate) async fn follow(
        &self,
        request: bbs_pb::SetEdgeRequest,
    ) -> Result<bbs_pb::SocialContext, UpstreamError> {
        let notify = request.edge == bbs_pb::SocialEdgeType::Follow as i32
            && request.active
            && request.target_user_id != request.user_id;
        let source = request.user_id.clone();
        let recipient = request.target_user_id.clone();
        let context = grpc_call!(self, bbs, "bbs", set_edge, request)?;
        if notify {
            self.create_community_notification(
                &recipient,
                notification(
                    format!("follow:{source}:{recipient}"),
                    "收到一个新关注",
                    "有人开始关注你",
                    [
                        ("actor_id", source.as_str()),
                        ("target_user_id", recipient.as_str()),
                        ("interaction", "follow"),
                    ],
                ),
            )
            .await;
        }
        Ok(context)
    }

    pub(crate) async fn ad_decisions(
        &self,
        request: ad_main_pb::DecisionRequest,
    ) -> Result<ad_main_pb::DecisionResponse, UpstreamError> {
        grpc_call!(self, ad_main, "ad-main", decide, request)
    }

    pub(crate) async fn advertiser_campaigns(
        &self,
        request: ad_center_pb::AdvertiserCampaignQuery,
    ) -> Result<ad_center_pb::CampaignList, UpstreamError> {
        grpc_call!(self, ad_center, "ad-center", campaigns, request)
    }

    pub(crate) async fn advertiser_campaign(
        &self,
        request: ad_center_pb::CampaignIdRequest,
    ) -> Result<ad_center_pb::AdCampaign, UpstreamError> {
        grpc_call!(self, ad_center, "ad-center", campaign, request)
    }

    pub(crate) async fn create_advertiser_campaign(
        &self,
        request: ad_center_pb::CreateCampaignRequest,
    ) -> Result<ad_center_pb::AdCampaign, UpstreamError> {
        grpc_call!(self, ad_center, "ad-center", create_campaign, request)
    }

    pub(crate) async fn update_advertiser_campaign(
        &self,
        request: ad_center_pb::UpdateCampaignRequest,
    ) -> Result<ad_center_pb::AdCampaign, UpstreamError> {
        grpc_call!(self, ad_center, "ad-center", update_campaign, request)
    }

    pub(crate) async fn report_ad_event(
        &self,
        request: ad_center_pb::RecordEventRequest,
    ) -> Result<ad_center_pb::EventReceipt, UpstreamError> {
        grpc_call!(self, ad_main, "ad-main", report_event, request)
    }

    pub(crate) async fn mall_products(
        &self,
        request: mall_pb::ProductQueryRequest,
    ) -> Result<mall_pb::ProductPage, UpstreamError> {
        grpc_call!(self, mall, "mall", products, request)
    }

    pub(crate) async fn create_mall_product(
        &self,
        request: mall_pb::CreateProductRequest,
    ) -> Result<mall_pb::MallProduct, UpstreamError> {
        grpc_call!(self, mall, "mall", create_product, request)
    }

    pub(crate) async fn update_mall_product(
        &self,
        request: mall_pb::UpdateProductRequest,
    ) -> Result<mall_pb::MallProduct, UpstreamError> {
        grpc_call!(self, mall, "mall", update_product, request)
    }

    pub(crate) async fn attach_mall_node_offer(
        &self,
        request: mall_pb::AttachNodeOfferRequest,
    ) -> Result<mall_pb::NodeOffer, UpstreamError> {
        grpc_call!(self, mall, "mall", attach_node_offer, request)
    }

    pub(crate) async fn mall_node_offers(
        &self,
        request: mall_pb::NodeOfferQueryRequest,
    ) -> Result<mall_pb::NodeOfferList, UpstreamError> {
        grpc_call!(self, mall, "mall", node_offers, request)
    }

    pub(crate) async fn create_mall_order(
        &self,
        request: mall_order_pb::CreateRequest,
    ) -> Result<mall_order_pb::Order, UpstreamError> {
        if request.idempotency_key.trim().is_empty() {
            return Err(upstream_invalid(
                "Idempotency-Key is required when creating an order",
            ));
        }
        grpc_call!(self, mall_order, "mall-order", create, request)
    }

    pub(crate) async fn mall_orders(
        &self,
        request: mall_order_pb::UserRequest,
    ) -> Result<mall_order_pb::OrderListResponse, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", list, request)
    }

    pub(crate) async fn mall_order(
        &self,
        request: mall_order_pb::OrderRequest,
    ) -> Result<mall_order_pb::Order, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", get, request)
    }

    pub(crate) async fn cancel_mall_order(
        &self,
        request: mall_order_pb::OrderRequest,
    ) -> Result<mall_order_pb::Order, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", cancel, request)
    }

    pub(crate) async fn merchant_mall_orders(
        &self,
        request: mall_order_pb::MerchantOrderRequest,
    ) -> Result<mall_order_pb::MerchantOrderListResponse, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", merchant_orders, request)
    }

    pub(crate) async fn update_mall_fulfillment(
        &self,
        request: mall_order_pb::UpdateFulfillmentRequest,
    ) -> Result<mall_order_pb::Order, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", update_fulfillment, request)
    }

    pub(crate) async fn affiliate_settlements(
        &self,
        request: mall_order_pb::AffiliateSettlementRequest,
    ) -> Result<mall_order_pb::AffiliateSettlementListResponse, UpstreamError> {
        grpc_call!(
            self,
            mall_order,
            "mall-order",
            affiliate_settlements,
            request
        )
    }

    pub(crate) async fn settle_affiliate(
        &self,
        request: mall_order_pb::SettleAffiliateRequest,
    ) -> Result<mall_order_pb::AffiliateSettlement, UpstreamError> {
        grpc_call!(self, mall_order, "mall-order", settle_affiliate, request)
    }

    async fn visibility(&self, user_id: &str) -> Result<Vec<String>, UpstreamError> {
        let visibility = grpc_call!(
            self,
            bbs,
            "bbs",
            visibility_context,
            bbs_pb::ContextRequest {
                user_id: user_id.to_string(),
                post_ids: Vec::new(),
            }
        )?;
        Ok(visibility
            .excluded_author_ids
            .into_iter()
            .map(|author_id| author_id.trim().to_string())
            .filter(|author_id| !author_id.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    async fn enrich_search_creators(
        &self,
        response: &mut search_pb::SearchResponse,
        excluded_author_ids: &[String],
    ) -> Vec<creator_pb::CreatorProfile> {
        let creator_ids = search_creator_ids(&response.items);
        if creator_ids.is_empty() {
            return Vec::new();
        }
        let page = self
            .creator_profiles(creator_pb::ListCreatorProfilesRequest {
                user_ids: creator_ids,
                query: None,
                specialty: None,
                cursor: None,
                limit: Some(50),
                excluded_user_ids: excluded_author_ids.to_vec(),
            })
            .await;
        let page = match page {
            Ok(page) => page,
            Err(error) => {
                // Creator enrichment must not make the core search path unavailable.
                response.degraded = true;
                tracing::warn!(%error, "search creator enrichment degraded");
                return Vec::new();
            }
        };
        let paused_creator_ids = page
            .items
            .iter()
            .filter(|profile| profile.state == creator_pb::CreatorState::Paused as i32)
            .map(|profile| profile.user_id.as_str())
            .collect::<BTreeSet<_>>();
        if !paused_creator_ids.is_empty() {
            let removed = remove_paused_creator_results(&mut response.items, &paused_creator_ids);
            response.total_estimate = response.total_estimate.saturating_sub(removed as u64);
        }
        page.items
            .into_iter()
            .filter(|profile| profile.state == creator_pb::CreatorState::Active as i32)
            .collect()
    }

    async fn public_content_for_viewer(
        &self,
        user_id: &str,
        content_id: &str,
    ) -> Result<bbs_link_pb::Content, UpstreamError> {
        let content = grpc_call!(
            self,
            bbs_link,
            "bbs-link",
            get_public,
            bbs_link_pb::IdRequest {
                id: content_id.to_string(),
            }
        )?;
        if self.visibility(user_id).await?.contains(&content.author_id) {
            return Err(hidden_content());
        }
        Ok(content)
    }

    async fn create_community_notification(
        &self,
        recipient_user_id: &str,
        request: growth_pb::CreateNotificationRequest,
    ) {
        match &self.community_notifications {
            CommunityNotificationSink::Postgres(Dao) => {
                let job = match community_notification_job(recipient_user_id, request) {
                    Ok(job) => job,
                    Err(error) => {
                        tracing::error!(%error, recipient_user_id, "could not serialize community notification job");
                        return;
                    }
                };
                if let Err(error) = Dao.enqueue(job).await {
                    // The interaction is already owned and committed by another
                    // service, so do not turn a successful user action into an
                    // ambiguous failure. Operations must alert on this signal.
                    tracing::warn!(%error, recipient_user_id, "community notification enqueue degraded");
                }
            }
            CommunityNotificationSink::Direct => {
                self.deliver_community_notification(recipient_user_id, request)
                    .await;
            }
        }
    }

    async fn mark_content_bookmarked(
        &self,
        user_id: &str,
        content_id: &str,
    ) -> Result<(), UpstreamError> {
        grpc_call!(
            self,
            interaction_status,
            "interaction-status",
            set_reaction,
            like_pb::SetReactionRequest {
                user_id: user_id.to_string(),
                post_id: content_id.to_string(),
                reaction: like_pb::ReactionType::Bookmark as i32,
                active: true,
            }
        )
        .map(|_| ())
    }

    async fn record_content_knowledge_capture(
        &self,
        user_id: &str,
        content_id: &str,
        attribution: Option<&ContentAttribution>,
    ) -> Result<(), UpstreamError> {
        let event = content_knowledge_capture_event(user_id, content_id, attribution).ok_or(
            UpstreamError::Transport {
                service: "user-event",
                message: "failed to format a knowledge capture timestamp".to_string(),
            },
        )?;
        grpc_call!(
            self,
            user_event,
            "user-event",
            ingest,
            user_event_pb::IngestRequest {
                user_id: user_id.to_string(),
                events: vec![event],
            }
        )?;
        Ok(())
    }

    async fn deliver_community_notification(
        &self,
        recipient_user_id: &str,
        mut request: growth_pb::CreateNotificationRequest,
    ) {
        request.user_id = recipient_user_id.to_string();
        let mut client = self.growth.clone();
        let result = match service_request("growth", request) {
            Ok(request) => client
                .create_notification(request)
                .await
                .map(tonic::Response::into_inner)
                .map_err(|error| grpc_error("growth", error)),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            tracing::warn!(%error, recipient_user_id, "community notification degraded");
        }
    }

    async fn record_route_action_completion(
        &self,
        user_id: &str,
        source_route_id: &str,
        action_id: &str,
    ) -> Result<(), UpstreamError> {
        let event = route_action_completion_event(user_id, source_route_id, action_id).ok_or(
            UpstreamError::Transport {
                service: "user-event",
                message: "failed to format a route completion timestamp".to_string(),
            },
        )?;
        grpc_call!(
            self,
            user_event,
            "user-event",
            ingest,
            user_event_pb::IngestRequest {
                user_id: user_id.to_string(),
                events: vec![event],
            }
        )?;
        Ok(())
    }

    async fn record_route_join(
        &self,
        user_id: &str,
        route_id: &str,
        attribution: Option<&ContentAttribution>,
    ) -> Result<(), UpstreamError> {
        let event =
            route_join_event(user_id, route_id, attribution).ok_or(UpstreamError::Transport {
                service: "user-event",
                message: "failed to format a route join timestamp".to_string(),
            })?;
        grpc_call!(
            self,
            user_event,
            "user-event",
            ingest,
            user_event_pb::IngestRequest {
                user_id: user_id.to_string(),
                events: vec![event],
            }
        )?;
        Ok(())
    }

    async fn record_reaction_signal(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: like_pb::ReactionType,
        negative_feedback_reason: Option<user_event_pb::NegativeFeedbackReason>,
        attribution: Option<&ContentAttribution>,
    ) -> Result<(), UpstreamError> {
        let event = reaction_signal_event(
            user_id,
            post_id,
            reaction,
            negative_feedback_reason,
            attribution,
        )
        .ok_or(UpstreamError::Transport {
            service: "user-event",
            message: "failed to format a reaction timestamp".to_string(),
        })?;
        grpc_call!(
            self,
            user_event,
            "user-event",
            ingest,
            user_event_pb::IngestRequest {
                user_id: user_id.to_string(),
                events: vec![event],
            }
        )?;
        Ok(())
    }

    async fn record_knowledge_action_completion(
        &self,
        user_id: &str,
        source_content_id: &str,
        action_id: &str,
    ) -> Result<(), UpstreamError> {
        let event = knowledge_action_completion_event(user_id, source_content_id, action_id)
            .ok_or(UpstreamError::Transport {
                service: "user-event",
                message: "failed to format a knowledge completion timestamp".to_string(),
            })?;
        grpc_call!(
            self,
            user_event,
            "user-event",
            ingest,
            user_event_pb::IngestRequest {
                user_id: user_id.to_string(),
                events: vec![event],
            }
        )?;
        Ok(())
    }
}

fn route_action_completion_event(
    user_id: &str,
    source_route_id: &str,
    action_id: &str,
) -> Option<user_event_pb::Event> {
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key =
        format!("bookway:route-action-complete:{user_id}:{source_route_id}:{action_id}");
    Some(user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: "complete".to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: "route-action-complete".to_string(),
        content_id: Some(source_route_id.to_string()),
        position: None,
        occurred_at,
        source: "gateway-route-completion".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason: None,
    })
}

fn route_join_event(
    user_id: &str,
    route_id: &str,
    attribution: Option<&ContentAttribution>,
) -> Option<user_event_pb::Event> {
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key = format!("bookway:route-join:{user_id}:{route_id}");
    let mut event = user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: "join_route".to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: "route-join".to_string(),
        content_id: Some(route_id.to_string()),
        position: None,
        occurred_at,
        source: "gateway-route-join".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason: None,
    };
    apply_content_attribution(&mut event, attribution);
    Some(event)
}

fn reaction_signal_event(
    user_id: &str,
    post_id: &str,
    reaction: like_pb::ReactionType,
    negative_feedback_reason: Option<user_event_pb::NegativeFeedbackReason>,
    attribution: Option<&ContentAttribution>,
) -> Option<user_event_pb::Event> {
    let (event_type, component_id, negative_feedback_reason) = match reaction {
        like_pb::ReactionType::Like => ("like", "post-like", None),
        like_pb::ReactionType::Bookmark => ("bookmark", "post-bookmark", None),
        like_pb::ReactionType::Hide => (
            "hide",
            "post-hide",
            negative_feedback_reason.map(|reason| reason as i32),
        ),
    };
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key = format!("bookway:reaction:{event_type}:{user_id}:{post_id}");
    let mut event = user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: event_type.to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: component_id.to_string(),
        content_id: Some(post_id.to_string()),
        position: None,
        occurred_at,
        source: "gateway-reaction".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason,
    };
    apply_content_attribution(&mut event, attribution);
    Some(event)
}

fn knowledge_action_completion_event(
    user_id: &str,
    source_content_id: &str,
    action_id: &str,
) -> Option<user_event_pb::Event> {
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key =
        format!("bookway:knowledge-action-complete:{user_id}:{source_content_id}:{action_id}");
    Some(user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: "complete".to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: "knowledge-action-complete".to_string(),
        content_id: Some(source_content_id.to_string()),
        position: None,
        occurred_at,
        source: "gateway-knowledge-completion".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason: None,
    })
}

fn content_knowledge_capture_event(
    user_id: &str,
    content_id: &str,
    attribution: Option<&ContentAttribution>,
) -> Option<user_event_pb::Event> {
    let occurred_at = OffsetDateTime::now_utc().format(&Rfc3339).ok()?;
    let stable_key = format!("bookway:knowledge-capture:{user_id}:{content_id}");
    let mut event = user_event_pb::Event {
        event_id: Uuid::new_v5(&Uuid::NAMESPACE_URL, stable_key.as_bytes()).to_string(),
        event_type: "save_knowledge".to_string(),
        session_id: "server".to_string(),
        request_id: None,
        component_id: "knowledge-capture".to_string(),
        content_id: Some(content_id.to_string()),
        position: None,
        occurred_at,
        source: "gateway-knowledge-capture".to_string(),
        attribution_source: user_event_pb::AttributionSource::Unspecified as i32,
        negative_feedback_reason: None,
    };
    apply_content_attribution(&mut event, attribution);
    Some(event)
}

fn apply_content_attribution(
    event: &mut user_event_pb::Event,
    attribution: Option<&ContentAttribution>,
) {
    let Some(attribution) = attribution else {
        return;
    };
    event.session_id = attribution.session_id.clone();
    event.request_id = Some(attribution.request_id.clone());
    event.position = Some(attribution.position);
    event.attribution_source = attribution.source as i32;
}

fn search_creator_ids(items: &[search_pb::SearchResult]) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.result_type == search_pb::SearchResultType::User as i32)
        .map(|item| {
            item.author_id
                .as_deref()
                .unwrap_or(&item.id)
                .trim()
                .to_string()
        })
        .filter(|user_id| !user_id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        // Creator's batch profile contract caps a request at fifty IDs.
        .take(50)
        .collect()
}

fn remove_paused_creator_results(
    items: &mut Vec<search_pb::SearchResult>,
    paused_creator_ids: &BTreeSet<&str>,
) -> usize {
    let original_len = items.len();
    items.retain(|item| {
        item.result_type != search_pb::SearchResultType::User as i32
            || !paused_creator_ids.contains(item.author_id.as_deref().unwrap_or(&item.id))
    });
    original_len.saturating_sub(items.len())
}

fn service_request<T>(
    service: &'static str,
    message: T,
) -> Result<tonic::Request<T>, UpstreamError> {
    bookway_runtime::grpc_service_request(message).map_err(|error| UpstreamError::Transport {
        service,
        message: error.to_string(),
    })
}

fn grpc_error(service: &'static str, error: tonic::Status) -> UpstreamError {
    UpstreamError::Grpc {
        service,
        code: error.code(),
        message: error.to_string(),
    }
}

fn route_precondition(message: &str) -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::FailedPrecondition,
        message: message.to_string(),
    }
}

fn permission_denied(message: &str) -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::PermissionDenied,
        message: message.to_string(),
    }
}

fn upstream_invalid(message: &str) -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::InvalidArgument,
        message: message.to_string(),
    }
}

fn hidden_content() -> UpstreamError {
    UpstreamError::Grpc {
        service: "gateway",
        code: tonic::Code::NotFound,
        message: "content not found".to_string(),
    }
}

fn public_author_content_request(
    author_id: &str,
    cursor: Option<String>,
    limit: Option<u32>,
) -> bbs_link_pb::ListRequest {
    bbs_link_pb::ListRequest {
        cursor,
        limit,
        status: Some(bbs_link_pb::ContentStatus::Published as i32),
        strategy: Some("fresh".to_string()),
        ids: None,
        author_id: Some(author_id.to_string()),
        content_type: None,
        domain: None,
        author_ids: Vec::new(),
    }
}

fn public_route(
    content: &bbs_link_pb::Content,
) -> Result<&bbs_link_pb::PostSummary, UpstreamError> {
    if content.status != bbs_link_pb::ContentStatus::Published as i32 {
        return Err(route_precondition("只能加入已公开发布的路线"));
    }
    if content.content_type != bbs_link_pb::ContentType::Route as i32 {
        return Err(route_precondition("该内容不是可加入的路线"));
    }
    let post = content
        .post
        .as_ref()
        .ok_or_else(|| route_precondition("该内容没有可加入的路线"))?;
    Ok(post)
}

fn content_knowledge_request(
    content: &bbs_link_pb::Content,
) -> Result<growth_pb::CreateKnowledgeRequest, UpstreamError> {
    if content.status != bbs_link_pb::ContentStatus::Published as i32 {
        return Err(route_precondition("只能收集已公开发布的内容"));
    }
    let post = content
        .post
        .as_ref()
        .ok_or_else(|| route_precondition("该公开内容缺少可收集的摘要"))?;
    let title = capped_knowledge_text(&post.title, 200);
    if title.is_empty() {
        return Err(route_precondition("该公开内容缺少可收集的标题"));
    }
    let creator = if post.author_name.trim().is_empty() {
        capped_knowledge_text(&content.author_id, 120)
    } else {
        capped_knowledge_text(&post.author_name, 120)
    };
    Ok(growth_pb::CreateKnowledgeRequest {
        user_id: String::new(),
        // Source identity, rather than this key, is the durable uniqueness
        // rule. The key additionally makes a transport retry safe before the
        // source lookup is reached.
        idempotency_key: Some(format!("knowledge-capture:{}", content.id)),
        title,
        creator,
        summary: capped_knowledge_text(&post.summary, 1_000),
        kind: knowledge_kind_for_content(content.content_type),
        status: growth_pb::KnowledgeResourceStatus::Inbox as i32,
        source_url: Some(format!("bookway://content/{}", content.id)),
        // Do not copy mutable public text or assets into private storage. A
        // later read must use the canonical content endpoint and its policy.
        body: None,
        tags: knowledge_tags(content, post),
        journey_id: None,
        source_content_id: Some(content.id.clone()),
    })
}

fn public_resource_knowledge_request(
    user_id: &str,
    resource: &catalog_pb::Resource,
) -> Result<growth_pb::CreateKnowledgeRequest, UpstreamError> {
    if resource.status != catalog_pb::ResourceStatus::Published as i32 {
        return Err(route_precondition("只能收集已发布的公共资源"));
    }
    let title = capped_knowledge_text(&resource.title, 200);
    if title.is_empty() || resource.id.trim().is_empty() {
        return Err(route_precondition("公共资源缺少有效标题或标识"));
    }
    let kind = match catalog_pb::ResourceKind::try_from(resource.kind) {
        Ok(catalog_pb::ResourceKind::Book) => growth_pb::KnowledgeResourceKind::Book,
        Ok(catalog_pb::ResourceKind::Course) => growth_pb::KnowledgeResourceKind::Course,
        Ok(catalog_pb::ResourceKind::Article) => growth_pb::KnowledgeResourceKind::Article,
        Ok(catalog_pb::ResourceKind::Podcast) => growth_pb::KnowledgeResourceKind::Link,
        Ok(catalog_pb::ResourceKind::Tool) | Ok(catalog_pb::ResourceKind::Unspecified) | Err(_) => {
            growth_pb::KnowledgeResourceKind::Link
        }
    } as i32;
    let mut tags = Vec::new();
    for topic in &resource.topics {
        let topic = capped_knowledge_text(topic, 40);
        if !topic.is_empty()
            && !tags
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&topic))
        {
            tags.push(topic);
            if tags.len() == 20 {
                break;
            }
        }
    }
    Ok(growth_pb::CreateKnowledgeRequest {
        user_id: user_id.to_string(),
        idempotency_key: Some(format!("knowledge-catalog:{}", resource.id)),
        title,
        creator: capped_knowledge_text(&resource.provider, 120),
        summary: capped_knowledge_text(&resource.summary, 1_000),
        kind,
        status: growth_pb::KnowledgeResourceStatus::Inbox as i32,
        source_url: (!resource.url.trim().is_empty()).then(|| resource.url.clone()),
        body: None,
        tags,
        journey_id: None,
        source_content_id: None,
    })
}

fn knowledge_kind_for_content(content_type: i32) -> i32 {
    match bbs_link_pb::ContentType::try_from(content_type) {
        Ok(bbs_link_pb::ContentType::Article) => growth_pb::KnowledgeResourceKind::Article as i32,
        Ok(bbs_link_pb::ContentType::Video) => growth_pb::KnowledgeResourceKind::Video as i32,
        Ok(bbs_link_pb::ContentType::Note) => growth_pb::KnowledgeResourceKind::Note as i32,
        // A milestone is a public outcome note. Saving it keeps only a live
        // content reference, so its route evidence is never copied privately.
        Ok(bbs_link_pb::ContentType::Milestone) => growth_pb::KnowledgeResourceKind::Note as i32,
        Ok(bbs_link_pb::ContentType::Question) => growth_pb::KnowledgeResourceKind::Note as i32,
        // Routes stay executable through their dedicated adoption endpoint;
        // in a knowledge inbox they are a reference rather than another plan.
        Ok(bbs_link_pb::ContentType::Route) | Err(_) => {
            growth_pb::KnowledgeResourceKind::Link as i32
        }
    }
}

fn knowledge_tags(content: &bbs_link_pb::Content, post: &bbs_link_pb::PostSummary) -> Vec<String> {
    let mut tags = Vec::with_capacity(20);
    for tag in content.topics.iter().chain(post.tags.iter()) {
        let tag = capped_knowledge_text(tag, 40);
        if !tag.is_empty()
            && !tags
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&tag))
        {
            tags.push(tag);
            if tags.len() == 20 {
                break;
            }
        }
    }
    tags
}

fn capped_knowledge_text(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

fn route_journey_request(
    user_id: &str,
    route_id: &str,
    content: &bbs_link_pb::Content,
) -> Result<growth_pb::CreateRouteJourneyRequest, UpstreamError> {
    let post = public_route(content)?;
    let title = if post.route_title.trim().is_empty() {
        post.title.clone()
    } else {
        post.route_title.clone()
    };
    let duration_label = if post.route_duration.trim().is_empty() {
        "4 周".to_string()
    } else {
        post.route_duration.clone()
    };
    let domain = growth_pb::GrowthDomain::try_from(post.domain)
        .unwrap_or(growth_pb::GrowthDomain::Learning) as i32;
    let template = content
        .route_template
        .as_ref()
        .ok_or_else(|| route_precondition("该路线缺少结构化行动模板"))?;
    let first_action = template
        .actions
        .first()
        .ok_or_else(|| route_precondition("该路线缺少可执行行动"))?;
    let journey_type = match bbs_link_pb::RouteTemplateKind::try_from(template.journey_type) {
        Ok(bbs_link_pb::RouteTemplateKind::Project) => growth_pb::JourneyType::Project,
        Ok(bbs_link_pb::RouteTemplateKind::Habit) => growth_pb::JourneyType::Habit,
        Ok(bbs_link_pb::RouteTemplateKind::Quantity) => growth_pb::JourneyType::Quantity,
        Ok(bbs_link_pb::RouteTemplateKind::Travel) => growth_pb::JourneyType::Travel,
        Ok(bbs_link_pb::RouteTemplateKind::Challenge) => growth_pb::JourneyType::Challenge,
        Err(_) => return Err(route_precondition("该路线模板类型无效")),
    };
    let stages = template
        .stages
        .iter()
        .map(|stage| growth_pb::JourneyStageInput {
            title: stage.title.clone(),
            detail: stage.detail.clone(),
            completion_criteria: stage.completion_criteria.clone(),
        })
        .collect();
    let additional_actions = template
        .actions
        .iter()
        .skip(1)
        .map(|action| growth_pb::RouteActionTemplate {
            title: action.title.clone(),
            detail: action.detail.clone(),
            estimated_minutes: action.estimated_minutes,
            scheduled_label: action.scheduled_label.clone(),
            stage_index: action.stage_index,
        })
        .collect();
    Ok(growth_pb::CreateRouteJourneyRequest {
        user_id: user_id.to_string(),
        route_id: route_id.to_string(),
        journey: Some(growth_pb::CreateJourneyRequest {
            user_id: user_id.to_string(),
            title,
            intent: template.intent.clone(),
            domain,
            journey_type: journey_type as i32,
            completion_criteria: template.completion_criteria.clone(),
            stages,
            duration_label,
            first_action_title: first_action.title.clone(),
            first_action_detail: first_action.detail.clone(),
            estimated_minutes: first_action.estimated_minutes,
            first_action_scheduled_label: Some(first_action.scheduled_label.clone()),
            first_action_scheduled_for: None,
            first_action_scheduled_timezone: None,
            first_action_stage_index: first_action.stage_index,
            first_action_recurrence: None,
            idempotency_key: None,
        }),
        additional_actions,
    })
}

fn notification<const N: usize>(
    source_id: String,
    title: &str,
    body: &str,
    data: [(&str, &str); N],
) -> growth_pb::CreateNotificationRequest {
    growth_pb::CreateNotificationRequest {
        user_id: String::new(),
        kind: growth_pb::NotificationKind::Community as i32,
        source_id,
        title: title.to_string(),
        body: body.to_string(),
        data: data
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
    }
}

fn community_notification_job(
    recipient_user_id: &str,
    request: growth_pb::CreateNotificationRequest,
) -> Result<CommunityNotificationJob, serde_json::Error> {
    Ok(CommunityNotificationJob {
        source_id: request.source_id,
        recipient_user_id: recipient_user_id.to_string(),
        title: request.title,
        body: request.body,
        data: serde_json::to_value(request.data)?,
    })
}

fn comment_notifications(
    post_author_id: &str,
    comment: &comment_pb::CommentItem,
    parent_author_id: Option<&str>,
    actor_id: &str,
    post_id: &str,
) -> Vec<(String, growth_pb::CreateNotificationRequest)> {
    if comment.status != comment_pb::CommentStatus::Published as i32 {
        return Vec::new();
    }
    let mut notifications = Vec::with_capacity(2);
    if post_author_id != actor_id {
        notifications.push((
            post_author_id.to_string(),
            notification(
                format!("comment-post:{}:{post_author_id}", comment.id),
                "收到一条评论",
                "有人评论了你的内容",
                [
                    ("post_id", post_id),
                    ("comment_id", comment.id.as_str()),
                    ("actor_id", actor_id),
                    ("interaction", "comment"),
                ],
            ),
        ));
    }
    if let Some(parent_author_id) = parent_author_id
        && parent_author_id != actor_id
        && parent_author_id != post_author_id
    {
        notifications.push((
            parent_author_id.to_string(),
            notification(
                format!("comment-reply:{}:{parent_author_id}", comment.id),
                "收到一条回复",
                "有人回复了你的评论",
                [
                    ("post_id", post_id),
                    ("comment_id", comment.id.as_str()),
                    (
                        "parent_comment_id",
                        comment.parent_id.as_deref().unwrap_or_default(),
                    ),
                    ("actor_id", actor_id),
                    ("interaction", "comment_reply"),
                ],
            ),
        ));
    }
    notifications
}

fn comment_moderation_notification(
    comment: &comment_pb::CommentItem,
) -> growth_pb::CreateNotificationRequest {
    let (title, body, status) = match comment_pb::CommentStatus::try_from(comment.status) {
        Ok(comment_pb::CommentStatus::Published) => {
            ("评论已通过审核", "你的评论现已公开", "published")
        }
        Ok(comment_pb::CommentStatus::Restricted) => {
            ("评论未公开", "你的评论未能通过审核", "restricted")
        }
        _ => ("评论审核已更新", "你的评论审核状态已更新", "unknown"),
    };
    notification(
        format!("comment-moderation:{}", comment.id),
        title,
        body,
        [
            ("post_id", comment.post_id.as_str()),
            ("comment_id", comment.id.as_str()),
            ("status", status),
            ("interaction", "comment_moderation"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public_route_content(
        route_template: Option<bbs_link_pb::RouteTemplate>,
    ) -> bbs_link_pb::Content {
        bbs_link_pb::Content {
            post: Some(bbs_link_pb::PostSummary {
                id: "route-content".to_string(),
                title: "四周主题阅读".to_string(),
                summary: "从一个问题开始，读完再回到生活。".to_string(),
                domain: bbs_link_pb::GrowthDomain::Learning as i32,
                route_title: "四周主题阅读".to_string(),
                route_duration: "4 周".to_string(),
                ..Default::default()
            }),
            content_type: bbs_link_pb::ContentType::Route as i32,
            status: bbs_link_pb::ContentStatus::Published as i32,
            route_template,
            ..Default::default()
        }
    }

    fn user_search_result(id: &str, author_id: Option<&str>) -> search_pb::SearchResult {
        search_pb::SearchResult {
            id: id.to_string(),
            result_type: search_pb::SearchResultType::User as i32,
            author_id: author_id.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn search_creator_enrichment_only_batches_unique_user_results() {
        let items = vec![
            user_search_result("creator-a", Some("creator-a")),
            user_search_result("creator-a-alias", Some("creator-a")),
            user_search_result("creator-b", None),
            search_pb::SearchResult {
                id: "post-a".to_string(),
                result_type: search_pb::SearchResultType::Post as i32,
                author_id: Some("creator-post".to_string()),
                ..Default::default()
            },
        ];

        assert_eq!(
            search_creator_ids(&items),
            vec!["creator-a".to_string(), "creator-b".to_string()]
        );
    }

    #[test]
    fn paused_creator_profiles_do_not_leak_through_user_search_results() {
        let mut items = vec![
            user_search_result("creator-active", Some("creator-active")),
            user_search_result("creator-paused", Some("creator-paused")),
            search_pb::SearchResult {
                id: "post-by-paused".to_string(),
                result_type: search_pb::SearchResultType::Post as i32,
                author_id: Some("creator-paused".to_string()),
                ..Default::default()
            },
        ];
        let paused = BTreeSet::from(["creator-paused"]);

        let removed = remove_paused_creator_results(&mut items, &paused);

        assert_eq!(removed, 1);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.id != "creator-paused"));
        assert!(items.iter().any(|item| item.id == "post-by-paused"));
    }

    #[test]
    fn community_notification_job_preserves_the_growth_idempotency_key() {
        let job = community_notification_job(
            "author",
            notification(
                "like:reader:post-1".to_string(),
                "收到一个赞",
                "有人赞了你的内容",
                [("actor_id", "reader"), ("post_id", "post-1")],
            ),
        )
        .expect("notification string data serializes to JSON");

        assert_eq!(job.source_id, "like:reader:post-1");
        assert_eq!(job.recipient_user_id, "author");
        assert_eq!(job.data["actor_id"], "reader");
    }

    #[test]
    fn approved_comment_notifications_keep_the_original_comment_author() {
        let comment = comment_pb::CommentItem {
            id: "comment-1".to_string(),
            post_id: "post-1".to_string(),
            author_id: "reader-a".to_string(),
            author_name: "reader-a".to_string(),
            body: "这条评论通过了人工审核".to_string(),
            parent_id: Some("parent-1".to_string()),
            like_count: 0,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            status: comment_pb::CommentStatus::Published as i32,
        };

        let notifications = comment_notifications(
            "post-author",
            &comment,
            Some("parent-author"),
            &comment.author_id,
            &comment.post_id,
        );

        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[0].0, "post-author");
        assert_eq!(notifications[0].1.data["actor_id"], "reader-a");
        assert_eq!(notifications[1].0, "parent-author");
        assert_eq!(notifications[1].1.data["actor_id"], "reader-a");
    }

    #[test]
    fn manual_comment_decision_notifies_its_author_with_a_stable_source() {
        let notification = comment_moderation_notification(&comment_pb::CommentItem {
            id: "comment-1".to_string(),
            post_id: "post-1".to_string(),
            author_id: "reader-a".to_string(),
            author_name: "reader-a".to_string(),
            body: "这条评论已完成审核".to_string(),
            parent_id: None,
            like_count: 0,
            created_at: "2026-08-16T00:00:00Z".to_string(),
            status: comment_pb::CommentStatus::Restricted as i32,
        });

        assert_eq!(notification.source_id, "comment-moderation:comment-1");
        assert_eq!(notification.title, "评论未公开");
        assert_eq!(notification.data["status"], "restricted");
    }

    #[test]
    fn public_content_capture_is_a_metadata_only_private_reference() {
        let mut content = public_route_content(None);
        content.id = "article-city".to_string();
        content.author_id = "writer-a".to_string();
        content.content_type = bbs_link_pb::ContentType::Article as i32;
        content.body = "正文不得被复制到私有知识库".to_string();
        content.topics = vec!["城市".to_string(), "阅读".to_string()];
        let post = content.post.as_mut().expect("public summary");
        post.author_name = "作家 A".to_string();
        post.title = "如何用步行理解城市".to_string();
        post.summary = "从街区、节奏和观察开始".to_string();
        post.tags = vec!["阅读".to_string(), "散步".to_string()];

        let request = content_knowledge_request(&content).expect("public content is capturable");

        assert_eq!(request.source_content_id.as_deref(), Some("article-city"));
        assert_eq!(
            request.source_url.as_deref(),
            Some("bookway://content/article-city")
        );
        assert_eq!(
            request.kind,
            growth_pb::KnowledgeResourceKind::Article as i32
        );
        assert!(request.body.is_none());
        assert_eq!(request.tags, vec!["城市", "阅读", "散步"]);
    }

    #[test]
    fn published_catalog_resource_becomes_a_metadata_only_private_reference() {
        let request = public_resource_knowledge_request(
            "reader-a",
            &catalog_pb::Resource {
                id: "resource-city-walk".to_string(),
                title: "城市步行方法".to_string(),
                kind: catalog_pb::ResourceKind::Course as i32,
                provider: "Bookway Academy".to_string(),
                summary: "从观察街区开始建立自己的步行路线".to_string(),
                url: "https://example.test/city-walk".to_string(),
                license: "CC BY 4.0".to_string(),
                version: "1".to_string(),
                citation: "Bookway Academy (2026)".to_string(),
                topics: vec!["城市".to_string(), "步行".to_string(), "城市".to_string()],
                status: catalog_pb::ResourceStatus::Published as i32,
                published_at: "2026-08-18T00:00:00Z".to_string(),
                updated_at: "2026-08-18T00:00:00Z".to_string(),
            },
        )
        .expect("published resource is capturable");

        assert_eq!(request.user_id, "reader-a");
        assert_eq!(
            request.idempotency_key.as_deref(),
            Some("knowledge-catalog:resource-city-walk")
        );
        assert_eq!(
            request.kind,
            growth_pb::KnowledgeResourceKind::Course as i32
        );
        assert_eq!(
            request.source_url.as_deref(),
            Some("https://example.test/city-walk")
        );
        assert!(request.body.is_none());
        assert!(request.source_content_id.is_none());
        assert_eq!(request.tags, vec!["城市", "步行"]);
    }

    #[test]
    fn content_knowledge_capture_event_has_a_stable_identity() {
        let first = content_knowledge_capture_event("user-1", "post-1", None)
            .expect("current timestamp should be serializable");
        let retry = content_knowledge_capture_event("user-1", "post-1", None)
            .expect("current timestamp should be serializable");

        assert_eq!(first.event_id, retry.event_id);
        assert_eq!(first.event_type, "save_knowledge");
        assert_eq!(first.content_id.as_deref(), Some("post-1"));
    }

    #[test]
    fn route_join_event_has_a_stable_content_identity() {
        let first = route_join_event("user-1", "route-1", None)
            .expect("current timestamp should be serializable");
        let retry = route_join_event("user-1", "route-1", None)
            .expect("current timestamp should be serializable");

        assert_eq!(first.event_id, retry.event_id);
        assert_eq!(first.event_type, "join_route");
        assert_eq!(first.component_id, "route-join");
        assert_eq!(first.content_id.as_deref(), Some("route-1"));
    }

    #[test]
    fn reaction_signal_events_are_stable_and_preserve_typed_hide_feedback() {
        let first =
            reaction_signal_event("user-1", "post-1", like_pb::ReactionType::Like, None, None)
                .expect("current timestamp should be serializable");
        let retry =
            reaction_signal_event("user-1", "post-1", like_pb::ReactionType::Like, None, None)
                .expect("current timestamp should be serializable");
        let bookmark = reaction_signal_event(
            "user-1",
            "post-1",
            like_pb::ReactionType::Bookmark,
            None,
            None,
        )
        .expect("current timestamp should be serializable");
        let hide = reaction_signal_event(
            "user-1",
            "post-1",
            like_pb::ReactionType::Hide,
            Some(user_event_pb::NegativeFeedbackReason::LowQuality),
            None,
        )
        .expect("current timestamp should be serializable");
        let hide_retry = reaction_signal_event(
            "user-1",
            "post-1",
            like_pb::ReactionType::Hide,
            Some(user_event_pb::NegativeFeedbackReason::NotRelevant),
            None,
        )
        .expect("current timestamp should be serializable");

        assert_eq!(first.event_id, retry.event_id);
        assert_ne!(first.event_id, bookmark.event_id);
        assert_ne!(first.event_id, hide.event_id);
        assert_eq!(hide.event_id, hide_retry.event_id);
        assert_eq!(first.event_type, "like");
        assert_eq!(first.component_id, "post-like");
        assert_eq!(first.content_id.as_deref(), Some("post-1"));
        assert_eq!(bookmark.event_type, "bookmark");
        assert_eq!(bookmark.component_id, "post-bookmark");
        assert_eq!(hide.event_type, "hide");
        assert_eq!(hide.component_id, "post-hide");
        assert_eq!(
            hide.negative_feedback_reason,
            Some(user_event_pb::NegativeFeedbackReason::LowQuality as i32)
        );
    }

    #[test]
    fn conversion_events_preserve_the_served_exposure_context() {
        let attribution = ContentAttribution {
            session_id: "01980000-0000-7000-8000-000000000001".to_string(),
            request_id: "01980000-0000-7000-8000-000000000002".to_string(),
            position: 3,
            source: user_event_pb::AttributionSource::Search,
        };
        let event = content_knowledge_capture_event("user-1", "post-1", Some(&attribution))
            .expect("current timestamp should be serializable");

        assert_eq!(event.session_id, attribution.session_id);
        assert_eq!(
            event.request_id.as_deref(),
            Some(attribution.request_id.as_str())
        );
        assert_eq!(event.position, Some(3));
        assert_eq!(
            event.attribution_source,
            user_event_pb::AttributionSource::Search as i32
        );
    }

    #[test]
    fn knowledge_action_completion_has_a_stable_content_identity() {
        let first = knowledge_action_completion_event("user-1", "post-1", "action-1")
            .expect("current timestamp should be serializable");
        let retry = knowledge_action_completion_event("user-1", "post-1", "action-1")
            .expect("current timestamp should be serializable");

        assert_eq!(first.event_id, retry.event_id);
        assert_eq!(first.event_type, "complete");
        assert_eq!(first.component_id, "knowledge-action-complete");
        assert_eq!(first.content_id.as_deref(), Some("post-1"));
    }

    #[test]
    fn maps_a_public_route_template_to_a_private_growth_plan() {
        let request = route_journey_request(
            "user-1",
            "route-content",
            &public_route_content(Some(bbs_link_pb::RouteTemplate {
                intent: "建立不焦虑的阅读节奏".to_string(),
                completion_criteria: "完成四次主题阅读和一次回望".to_string(),
                stages: vec![
                    bbs_link_pb::RouteTemplateStage {
                        title: "选题".to_string(),
                        detail: "选择一个真实问题".to_string(),
                        completion_criteria: "确定阅读问题".to_string(),
                    },
                    bbs_link_pb::RouteTemplateStage {
                        title: "回望".to_string(),
                        detail: "把结论写成自己的话".to_string(),
                        completion_criteria: "写下一次调整".to_string(),
                    },
                ],
                actions: vec![
                    bbs_link_pb::RouteTemplateAction {
                        id: "reading-start".to_string(),
                        title: "选一本起步书".to_string(),
                        detail: "只选最相关的一本".to_string(),
                        estimated_minutes: 15,
                        scheduled_label: "今天".to_string(),
                        stage_index: Some(0),
                        scene_equipment: vec!["阅读清单".to_string()],
                    },
                    bbs_link_pb::RouteTemplateAction {
                        id: "reading-reflect".to_string(),
                        title: "写一段回望".to_string(),
                        detail: "记录这个方法是否适合自己".to_string(),
                        estimated_minutes: 20,
                        scheduled_label: "第四周".to_string(),
                        stage_index: Some(1),
                        scene_equipment: vec!["复盘笔记本".to_string()],
                    },
                ],
                journey_type: bbs_link_pb::RouteTemplateKind::Habit as i32,
            })),
        )
        .expect("public route template should be joinable");

        let journey = request.journey.expect("journey request is populated");
        assert_eq!(journey.journey_type, growth_pb::JourneyType::Habit as i32);
        assert_eq!(journey.stages.len(), 2);
        assert_eq!(journey.first_action_title, "选一本起步书");
        assert_eq!(journey.first_action_stage_index, Some(0));
        assert_eq!(request.additional_actions.len(), 1);
        assert_eq!(request.additional_actions[0].stage_index, Some(1));
    }

    #[test]
    fn adopted_action_completion_uses_a_stable_route_attribution_event() {
        let first = route_action_completion_event("user-1", "route-content", "action-1")
            .expect("current timestamp should be serializable");
        let retry = route_action_completion_event("user-1", "route-content", "action-1")
            .expect("current timestamp should be serializable");

        assert_eq!(first.event_id, retry.event_id);
        assert!(Uuid::parse_str(&first.event_id).is_ok());
        assert_eq!(first.event_type, "complete");
        assert_eq!(first.content_id.as_deref(), Some("route-content"));
        assert_eq!(first.request_id, None);
        assert_eq!(first.position, None);
    }

    #[test]
    fn routes_without_structured_actions_cannot_be_joined() {
        assert!(
            route_journey_request("user-1", "route-content", &public_route_content(None)).is_err()
        );
    }

    #[test]
    fn notes_cannot_be_joined_even_when_legacy_metadata_looks_like_a_route() {
        let mut note = public_route_content(None);
        note.content_type = bbs_link_pb::ContentType::Note as i32;

        assert!(public_route(&note).is_err());
    }

    #[test]
    fn public_author_content_query_only_requests_published_content() {
        let request =
            public_author_content_request("author-1", Some("cursor-1".to_string()), Some(20));

        assert_eq!(request.author_id.as_deref(), Some("author-1"));
        assert_eq!(request.cursor.as_deref(), Some("cursor-1"));
        assert_eq!(request.limit, Some(20));
        assert_eq!(
            request.status,
            Some(bbs_link_pb::ContentStatus::Published as i32)
        );
        assert_eq!(request.strategy.as_deref(), Some("fresh"));
        assert!(request.ids.is_none());
    }
}
