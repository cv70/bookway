#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_link_server::BbsLink};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsLink for GrpcServer {
    async fn list(
        &self,
        request: Request<pb::ListRequest>,
    ) -> Result<Response<pb::ContentPage>, Status> {
        Ok(Response::new(
            self.domain
                .list(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get_public_summaries(
        &self,
        request: Request<pb::PublicContentSummariesRequest>,
    ) -> Result<Response<pb::PublicContentSummaries>, Status> {
        Ok(Response::new(
            self.domain
                .get_public_summaries(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get(&self, request: Request<pb::IdRequest>) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .get(&request.into_inner().id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get_public(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .get_public(&request.into_inner().id)
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn create(
        &self,
        request: Request<pb::CreateRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .create(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn update(
        &self,
        request: Request<pb::UpdateRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .update(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn publish(
        &self,
        request: Request<pb::PublishRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .publish(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn restrict(
        &self,
        request: Request<pb::RestrictRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .restrict(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn restore(
        &self,
        request: Request<pb::RestoreRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .restore(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn accept_answer(
        &self,
        request: Request<pb::AcceptAnswerRequest>,
    ) -> Result<Response<pb::Content>, Status> {
        Ok(Response::new(
            self.domain
                .accept_answer(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_link_server::BbsLinkServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_link_server::BbsLinkServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn internal_error(error: crate::domain::ContentError) -> Status {
    match error {
        crate::domain::ContentError::Validation(message) => Status::invalid_argument(message),
        crate::domain::ContentError::Forbidden => {
            Status::permission_denied("content belongs to another author")
        }
        crate::domain::ContentError::Repository(crate::datasource::RepositoryError::NotFound(
            message,
        )) => Status::not_found(message),
        crate::domain::ContentError::Repository(
            crate::datasource::RepositoryError::IdempotencyConflict(message),
        ) => Status::already_exists(message),
        crate::domain::ContentError::Repository(
            crate::datasource::RepositoryError::VersionConflict,
        ) => Status::aborted("content version conflict"),
        crate::domain::ContentError::Audit(message) => Status::unavailable(message),
        crate::domain::ContentError::Media(message) => Status::unavailable(message),
        crate::domain::ContentError::Repository(error) => Status::internal(error.to_string()),
    }
}
