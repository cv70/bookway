#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, growth_server::Growth};
use crate::{
    api::{
        CreateActionRequest, CreateGrowthEntryRequest, CreateJourneyRequest,
        CreateKnowledgeResourceRequest, CreateUserNotificationRequest, KnowledgeQueryRequest,
        NotificationQueryRequest, RegisterPushDeviceRequest, UpdateActionRequest,
        UpdateJourneyRequest, UpdateKnowledgeResourceRequest, UpdateReminderPreferencesRequest,
    },
    domain::Domain,
};
use serde::Serialize;
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
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(
            &self
                .domain
                .list_journeys(&user_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_journey(
        &self,
        request: Request<pb::CreateJourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateJourneyRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_journey(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_route_journey(
        &self,
        request: Request<pb::CreateRouteJourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateJourneyRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_route_journey(&request.user_id, &request.route_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn set_route_participation_intent(
        &self,
        request: Request<pb::SetRouteParticipationIntentRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .set_route_participation_intent(
                    &request.user_id,
                    &request.route_id,
                    request.active,
                    (!request.private_journey_id.is_empty()).then_some(request.private_journey_id),
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn get_journey(
        &self,
        request: Request<pb::JourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .get_journey(&request.user_id, &request.journey_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_journey(
        &self,
        request: Request<pb::UpdateJourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateJourneyRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_journey(&request.user_id, &request.journey_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_action(
        &self,
        request: Request<pb::CreateActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateActionRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_action(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn today(
        &self,
        request: Request<pb::ScheduleRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .today_for(
                    &request.user_id,
                    (!request.local_date.is_empty()).then_some(request.local_date.as_str()),
                    (!request.timezone.is_empty()).then_some(request.timezone.as_str()),
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn complete_action(
        &self,
        request: Request<pb::CompleteActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .complete_action(&request.user_id, &request.action_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_action(
        &self,
        request: Request<pb::UpdateActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateActionRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_action(&request.user_id, &request.action_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn reminder_preferences(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(
            &self
                .domain
                .reminder_preferences(&user_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_reminder_preferences(
        &self,
        request: Request<pb::UpdateReminderPreferencesRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateReminderPreferencesRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_reminder_preferences(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn register_push_device(
        &self,
        request: Request<pb::RegisterPushDeviceRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: RegisterPushDeviceRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .register_push_device(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn revoke_push_device(
        &self,
        request: Request<pb::PushDeviceRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        self.domain
            .revoke_push_device(&request.user_id, &request.device_id)
            .await
            .map_err(internal_error)?;
        json_response(&serde_json::json!({}))
    }

    async fn list_notifications(
        &self,
        request: Request<pb::NotificationQueryRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: NotificationQueryRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .list_notifications(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn mark_notification_read(
        &self,
        request: Request<pb::NotificationRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .mark_notification_read(&request.user_id, &request.notification_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_notification(
        &self,
        request: Request<pb::CreateNotificationRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateUserNotificationRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_notification(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn list_entries(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(
            &self
                .domain
                .list_entries(&user_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_entry(
        &self,
        request: Request<pb::CreateEntryRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateGrowthEntryRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_entry(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn weekly_review(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(
            &self
                .domain
                .weekly_review(&user_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn companion(
        &self,
        request: Request<pb::ScheduleRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .companion_brief_for(
                    &request.user_id,
                    (!request.local_date.is_empty()).then_some(request.local_date.as_str()),
                    (!request.timezone.is_empty()).then_some(request.timezone.as_str()),
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn list_knowledge(
        &self,
        request: Request<pb::KnowledgeQueryRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: KnowledgeQueryRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .list_knowledge(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_knowledge(
        &self,
        request: Request<pb::CreateKnowledgeRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateKnowledgeResourceRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_knowledge(
                    &request.user_id,
                    payload,
                    (!request.idempotency_key.is_empty()).then_some(request.idempotency_key),
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_knowledge(
        &self,
        request: Request<pb::UpdateKnowledgeRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateKnowledgeResourceRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_knowledge(&request.user_id, &request.resource_id, payload)
                .await
                .map_err(internal_error)?,
        )
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
        .add_service(pb::growth_server::GrowthServer::new(GrpcServer { domain }))
        .serve(addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::GrowthError) -> Status {
    match error {
        crate::domain::GrowthError::Validation(message) => Status::invalid_argument(message),
        crate::domain::GrowthError::Repository(
            crate::datasource::RepositoryError::JourneyNotFound(message)
            | crate::datasource::RepositoryError::ActionNotFound(message)
            | crate::datasource::RepositoryError::NotificationNotFound(message)
            | crate::datasource::RepositoryError::EntryReferenceNotFound(message)
            | crate::datasource::RepositoryError::KnowledgeNotFound(message)
            | crate::datasource::RepositoryError::KnowledgeReferenceNotFound(message),
        ) => Status::not_found(message),
        crate::domain::GrowthError::Repository(
            crate::datasource::RepositoryError::IdempotencyConflict,
        ) => Status::already_exists("idempotency key was already used with different content"),
        crate::domain::GrowthError::Repository(
            crate::datasource::RepositoryError::NotificationSourceConflict(source_id),
        ) => Status::already_exists(format!(
            "notification source {source_id} was already assigned to a different user"
        )),
        crate::domain::GrowthError::Repository(error) => Status::internal(error.to_string()),
    }
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
