#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, comment_server::Comment};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Comment for GrpcServer {
    async fn list(
        &self,
        request: Request<pb::ListRequest>,
    ) -> Result<Response<pb::CommentPage>, Status> {
        Ok(Response::new(
            self.domain
                .list(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn create(
        &self,
        request: Request<pb::CreateRequest>,
    ) -> Result<Response<pb::CreateCommentResult>, Status> {
        Ok(Response::new(
            self.domain
                .create(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn delete(
        &self,
        request: Request<pb::DeleteRequest>,
    ) -> Result<Response<pb::DeleteResponse>, Status> {
        self.domain
            .delete(request.into_inner())
            .await
            .map_err(internal_error)?;
        Ok(Response::new(pb::DeleteResponse {}))
    }

    async fn list_moderation(
        &self,
        request: Request<pb::ListModerationRequest>,
    ) -> Result<Response<pb::ModerationCommentPage>, Status> {
        Ok(Response::new(
            self.domain
                .list_moderation(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn review(
        &self,
        request: Request<pb::ReviewCommentRequest>,
    ) -> Result<Response<pb::ReviewCommentResult>, Status> {
        Ok(Response::new(
            self.domain
                .review(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn report(
        &self,
        request: Request<pb::CreateCommentReportRequest>,
    ) -> Result<Response<pb::CommentReport>, Status> {
        Ok(Response::new(
            self.domain
                .report(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn list_reports(
        &self,
        request: Request<pb::ListCommentReportsRequest>,
    ) -> Result<Response<pb::CommentReportPage>, Status> {
        Ok(Response::new(
            self.domain
                .list_reports(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn review_report(
        &self,
        request: Request<pb::ReviewCommentReportRequest>,
    ) -> Result<Response<pb::CommentReport>, Status> {
        Ok(Response::new(
            self.domain
                .review_report(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn appeal(
        &self,
        request: Request<pb::CreateCommentAppealRequest>,
    ) -> Result<Response<pb::CommentAppeal>, Status> {
        Ok(Response::new(
            self.domain
                .appeal(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn list_appeals(
        &self,
        request: Request<pb::ListCommentAppealsRequest>,
    ) -> Result<Response<pb::CommentAppealPage>, Status> {
        Ok(Response::new(
            self.domain
                .list_appeals(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn review_appeal(
        &self,
        request: Request<pb::ReviewCommentAppealRequest>,
    ) -> Result<Response<pb::CommentAppeal>, Status> {
        Ok(Response::new(
            self.domain
                .review_appeal(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::comment_server::CommentServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::comment_server::CommentServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn internal_error(error: crate::domain::CommentError) -> Status {
    let message = error.to_string();
    match error {
        crate::domain::CommentError::Validation(_) => Status::invalid_argument(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::ReplyDepthExceeded,
        ) => Status::invalid_argument(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::ParentNotFound(_)
            | crate::datasource::RepositoryError::NotFound(_)
            | crate::datasource::RepositoryError::ReportNotFound(_)
            | crate::datasource::RepositoryError::AppealNotFound(_),
        ) => Status::not_found(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::IdempotencyConflict
            | crate::datasource::RepositoryError::ReportIdempotencyConflict
            | crate::datasource::RepositoryError::AppealIdempotencyConflict,
        ) => Status::already_exists(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::ModerationConflict
            | crate::datasource::RepositoryError::ReportConflict
            | crate::datasource::RepositoryError::AppealConflict,
        ) => Status::aborted(message),
        crate::domain::CommentError::Repository(crate::datasource::RepositoryError::SelfReport) => {
            Status::permission_denied(message)
        }
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::NotReportable(_)
            | crate::datasource::RepositoryError::ActionConflict,
        ) => Status::failed_precondition(message),
        crate::domain::CommentError::Repository(_) => Status::internal(message),
    }
}
