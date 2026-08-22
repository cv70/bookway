#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, growth_server::Growth};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Growth for GrpcServer {
    async fn list_journeys(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JourneyList>, Status> {
        let response = self
            .domain
            .list_journeys(&request.into_inner().user_id)
            .await
            .map_err(internal_error)?;
        Ok(Response::new(pb::JourneyList { items: response }))
    }

    async fn create_journey(
        &self,
        request: Request<pb::CreateJourneyRequest>,
    ) -> Result<Response<pb::Journey>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .create_journey(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn create_route_journey(
        &self,
        request: Request<pb::CreateRouteJourneyRequest>,
    ) -> Result<Response<pb::Journey>, Status> {
        let request = request.into_inner();
        let journey = request
            .journey
            .ok_or_else(|| Status::invalid_argument("journey is required"))?;
        Ok(Response::new(
            self.domain
                .create_route_journey(
                    &request.user_id,
                    &request.route_id,
                    journey,
                    request.additional_actions,
                )
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn set_route_participation_intent(
        &self,
        request: Request<pb::SetRouteParticipationIntentRequest>,
    ) -> Result<Response<pb::RouteParticipationIntent>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .set_route_participation_intent(
                    &request.user_id,
                    &request.route_id,
                    request.active,
                    request.private_journey_id,
                )
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get_journey(
        &self,
        request: Request<pb::JourneyRequest>,
    ) -> Result<Response<pb::JourneyDetail>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .get_journey(&request.user_id, &request.journey_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn update_journey(
        &self,
        request: Request<pb::UpdateJourneyRequest>,
    ) -> Result<Response<pb::Journey>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let journey_id = request.journey_id.clone();
        Ok(Response::new(
            self.domain
                .update_journey(&user_id, &journey_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn create_action(
        &self,
        request: Request<pb::CreateActionRequest>,
    ) -> Result<Response<pb::Action>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .create_action(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn today(
        &self,
        request: Request<pb::ScheduleRequest>,
    ) -> Result<Response<pb::TodaySummary>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .today_for(
                    &request.user_id,
                    request.local_date.as_deref(),
                    request.timezone.as_deref(),
                )
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn complete_action(
        &self,
        request: Request<pb::CompleteActionRequest>,
    ) -> Result<Response<pb::CompleteActionResponse>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .complete_action(&request.user_id, &request.action_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn update_action(
        &self,
        request: Request<pb::UpdateActionRequest>,
    ) -> Result<Response<pb::Action>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let action_id = request.action_id.clone();
        Ok(Response::new(
            self.domain
                .update_action(&user_id, &action_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn reminder_preferences(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::ReminderPreference>, Status> {
        Ok(Response::new(
            self.domain
                .reminder_preferences(&request.into_inner().user_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn update_reminder_preferences(
        &self,
        request: Request<pb::UpdateReminderPreferencesRequest>,
    ) -> Result<Response<pb::ReminderPreference>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .update_reminder_preferences(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn register_push_device(
        &self,
        request: Request<pb::RegisterPushDeviceRequest>,
    ) -> Result<Response<pb::PushDevice>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .register_push_device(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn revoke_push_device(
        &self,
        request: Request<pb::PushDeviceRequest>,
    ) -> Result<Response<pb::EmptyResponse>, Status> {
        let request = request.into_inner();
        self.domain
            .revoke_push_device(&request.user_id, &request.device_id)
            .await
            .map_err(internal_error)?;
        Ok(Response::new(pb::EmptyResponse {}))
    }

    async fn list_notifications(
        &self,
        request: Request<pb::NotificationQueryRequest>,
    ) -> Result<Response<pb::NotificationPage>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .list_notifications(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn create_notification(
        &self,
        request: Request<pb::CreateNotificationRequest>,
    ) -> Result<Response<pb::UserNotification>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .create_notification(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn mark_notification_read(
        &self,
        request: Request<pb::NotificationRequest>,
    ) -> Result<Response<pb::UserNotification>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .mark_notification_read(&request.user_id, &request.notification_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn list_entries(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::EntryList>, Status> {
        let entries = self
            .domain
            .list_entries(&request.into_inner().user_id)
            .await
            .map_err(internal_error)?;
        Ok(Response::new(pb::EntryList { items: entries }))
    }

    async fn create_entry(
        &self,
        request: Request<pb::CreateEntryRequest>,
    ) -> Result<Response<pb::GrowthEntry>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .create_entry(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn retry_entry_publication(
        &self,
        request: Request<pb::RetryEntryPublicationRequest>,
    ) -> Result<Response<pb::GrowthEntry>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .retry_entry_publication(&request.user_id, &request.entry_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn weekly_review(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::WeeklyReviewSummary>, Status> {
        Ok(Response::new(
            self.domain
                .weekly_review(&request.into_inner().user_id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn save_weekly_review(
        &self,
        request: Request<pb::SaveWeeklyReviewRequest>,
    ) -> Result<Response<pb::ReviewRecord>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .save_weekly_review(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn apply_weekly_review_adjustment(
        &self,
        request: Request<pb::ApplyWeeklyReviewAdjustmentRequest>,
    ) -> Result<Response<pb::ApplyWeeklyReviewAdjustmentResponse>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        Ok(Response::new(
            self.domain
                .apply_weekly_review_adjustment(&user_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn companion(
        &self,
        request: Request<pb::ScheduleRequest>,
    ) -> Result<Response<pb::CompanionBrief>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .companion_brief_for(
                    &request.user_id,
                    request.local_date.as_deref(),
                    request.timezone.as_deref(),
                )
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn list_knowledge(
        &self,
        request: Request<pb::KnowledgeQueryRequest>,
    ) -> Result<Response<pb::KnowledgeList>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let items = self
            .domain
            .list_knowledge(&user_id, request)
            .await
            .map_err(internal_error)?;
        Ok(Response::new(pb::KnowledgeList { items }))
    }

    async fn create_knowledge(
        &self,
        request: Request<pb::CreateKnowledgeRequest>,
    ) -> Result<Response<pb::KnowledgeResource>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let idempotency_key = request.idempotency_key.clone();
        Ok(Response::new(
            self.domain
                .create_knowledge(&user_id, request, idempotency_key)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn start_knowledge_journey(
        &self,
        request: Request<pb::StartKnowledgeJourneyRequest>,
    ) -> Result<Response<pb::KnowledgeJourney>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let resource_id = request.resource_id.clone();
        Ok(Response::new(
            self.domain
                .start_knowledge_journey(&user_id, &resource_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn update_knowledge(
        &self,
        request: Request<pb::UpdateKnowledgeRequest>,
    ) -> Result<Response<pb::KnowledgeResource>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.clone();
        let resource_id = request.resource_id.clone();
        Ok(Response::new(
            self.domain
                .update_knowledge(&user_id, &resource_id, request)
                .await
                .map_err(internal_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::growth_server::GrowthServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::growth_server::GrowthServer::with_interceptor(
            GrpcServer { domain },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(addr)
        .await
}

fn internal_error(error: crate::domain::GrowthError) -> Status {
    match error {
        crate::domain::GrowthError::Validation(message) => Status::invalid_argument(message),
        crate::domain::GrowthError::Repository(
            crate::datasource::DaoError::JourneyNotFound(message)
            | crate::datasource::DaoError::ActionNotFound(message)
            | crate::datasource::DaoError::NotificationNotFound(message)
            | crate::datasource::DaoError::EntryReferenceNotFound(message)
            | crate::datasource::DaoError::EntryNotFound(message)
            | crate::datasource::DaoError::KnowledgeNotFound(message)
            | crate::datasource::DaoError::KnowledgeReferenceNotFound(message)
            | crate::datasource::DaoError::ReviewNotFound(message),
        ) => Status::not_found(message),
        crate::domain::GrowthError::Repository(
            crate::datasource::DaoError::IdempotencyConflict,
        ) => Status::already_exists("idempotency key was already used with different content"),
        crate::domain::GrowthError::Repository(
            crate::datasource::DaoError::NotificationSourceConflict(source_id),
        ) => Status::already_exists(format!(
            "notification source {source_id} was already assigned to a different user"
        )),
        crate::domain::GrowthError::Repository(
            crate::datasource::DaoError::EntryPublicationNotRetryable,
        ) => Status::failed_precondition("entry publication cannot be retried yet"),
        crate::domain::GrowthError::Repository(
            crate::datasource::DaoError::ReviewAdjustmentNotFound(_)
            | crate::datasource::DaoError::ReviewAdjustmentStale,
        ) => Status::failed_precondition("review adjustment is no longer applicable"),
        crate::domain::GrowthError::Repository(error) => Status::internal(error.to_string()),
    }
}
