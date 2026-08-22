#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_message_server::BbsMessage};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsMessage for GrpcServer {
    async fn send(
        &self,
        request: Request<pb::SendDirectMessageRequest>,
    ) -> Result<Response<pb::DirectMessage>, Status> {
        Ok(Response::new(
            self.domain
                .send(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn list_conversations(
        &self,
        request: Request<pb::ListConversationsRequest>,
    ) -> Result<Response<pb::ConversationPage>, Status> {
        Ok(Response::new(
            self.domain
                .list_conversations(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn list_messages(
        &self,
        request: Request<pb::ListMessagesRequest>,
    ) -> Result<Response<pb::DirectMessagePage>, Status> {
        Ok(Response::new(
            self.domain
                .list_messages(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn mark_conversation_read(
        &self,
        request: Request<pb::MarkConversationReadRequest>,
    ) -> Result<Response<pb::MarkConversationReadResponse>, Status> {
        Ok(Response::new(
            self.domain
                .mark_conversation_read(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn get_preferences(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::DirectMessagePreferences>, Status> {
        Ok(Response::new(
            self.domain
                .get_preferences(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn update_preferences(
        &self,
        request: Request<pb::UpdateDirectMessagePreferencesRequest>,
    ) -> Result<Response<pb::DirectMessagePreferences>, Status> {
        Ok(Response::new(
            self.domain
                .update_preferences(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn report(
        &self,
        request: Request<pb::ReportDirectMessageRequest>,
    ) -> Result<Response<pb::DirectMessageReport>, Status> {
        Ok(Response::new(
            self.domain
                .report(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn list_reports(
        &self,
        request: Request<pb::ListDirectMessageReportsRequest>,
    ) -> Result<Response<pb::DirectMessageReportPage>, Status> {
        Ok(Response::new(
            self.domain
                .list_reports(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn review_report(
        &self,
        request: Request<pb::ReviewDirectMessageReportRequest>,
    ) -> Result<Response<pb::DirectMessageReport>, Status> {
        Ok(Response::new(
            self.domain
                .review_report(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_message_server::BbsMessageServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_message_server::BbsMessageServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn domain_error(error: crate::domain::MessageError) -> Status {
    match error {
        crate::domain::MessageError::Validation(message) => Status::invalid_argument(message),
        crate::domain::MessageError::Blocked => Status::permission_denied(error.to_string()),
        crate::domain::MessageError::RecipientUnavailable => {
            Status::failed_precondition(error.to_string())
        }
        crate::domain::MessageError::SenderRestricted => {
            Status::permission_denied(error.to_string())
        }
        crate::domain::MessageError::UnderReview => Status::failed_precondition(error.to_string()),
        crate::domain::MessageError::Restricted => Status::permission_denied(error.to_string()),
        crate::domain::MessageError::Audit(message) => Status::unavailable(message),
        crate::domain::MessageError::Repository(crate::datasource::DaoError::NotFound(_)) => {
            Status::not_found(error.to_string())
        }
        crate::domain::MessageError::Repository(crate::datasource::DaoError::NotParticipant) => {
            Status::permission_denied(error.to_string())
        }
        crate::domain::MessageError::Repository(
            crate::datasource::DaoError::IdempotencyConflict,
        ) => Status::already_exists(error.to_string()),
        crate::domain::MessageError::Repository(
            crate::datasource::DaoError::MessageNotFound(_)
            | crate::datasource::DaoError::ReportNotFound(_),
        ) => Status::not_found(error.to_string()),
        crate::domain::MessageError::Repository(
            crate::datasource::DaoError::NotMessageRecipient,
        ) => Status::permission_denied(error.to_string()),
        crate::domain::MessageError::Repository(
            crate::datasource::DaoError::ReportIdempotencyConflict,
        ) => Status::already_exists(error.to_string()),
        crate::domain::MessageError::Repository(crate::datasource::DaoError::ReportConflict) => {
            Status::aborted(error.to_string())
        }
        crate::domain::MessageError::Repository(crate::datasource::DaoError::SenderRestricted) => {
            Status::permission_denied(error.to_string())
        }
        crate::domain::MessageError::Upstream(message) => Status::unavailable(message),
        crate::domain::MessageError::Repository(_) => Status::internal(error.to_string()),
    }
}
