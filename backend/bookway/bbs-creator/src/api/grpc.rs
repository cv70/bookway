#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_creator_server::BbsCreator};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsCreator for GrpcServer {
    async fn get_profile(
        &self,
        request: Request<pb::CreatorProfileRequest>,
    ) -> Result<Response<pb::CreatorProfile>, Status> {
        Ok(Response::new(
            self.domain
                .get_profile(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn upsert_profile(
        &self,
        request: Request<pb::UpsertCreatorProfileRequest>,
    ) -> Result<Response<pb::CreatorProfile>, Status> {
        Ok(Response::new(
            self.domain
                .upsert_profile(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn list_profiles(
        &self,
        request: Request<pb::ListCreatorProfilesRequest>,
    ) -> Result<Response<pb::CreatorProfilePage>, Status> {
        Ok(Response::new(
            self.domain
                .list_profiles(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_creator_server::BbsCreatorServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_creator_server::BbsCreatorServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn domain_error(error: crate::domain::CreatorError) -> Status {
    match error {
        crate::domain::CreatorError::Validation(message) => Status::invalid_argument(message),
        crate::domain::CreatorError::Repository(crate::datasource::RepositoryError::NotFound(
            _,
        )) => Status::not_found(error.to_string()),
        crate::domain::CreatorError::Repository(
            crate::datasource::RepositoryError::HandleTaken(_),
        ) => Status::already_exists(error.to_string()),
        crate::domain::CreatorError::Repository(_) => Status::internal(error.to_string()),
    }
}
