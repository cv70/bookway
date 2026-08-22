#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_server::Bbs};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Bbs for GrpcServer {
    async fn context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::SocialContext>, Status> {
        Ok(Response::new(
            self.domain
                .context(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn visibility_context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::SocialVisibility>, Status> {
        Ok(Response::new(
            self.domain
                .visibility_context(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn set_edge(
        &self,
        request: Request<pb::SetEdgeRequest>,
    ) -> Result<Response<pb::SocialContext>, Status> {
        Ok(Response::new(
            self.domain
                .set_edge(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn list_route_participations(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::RouteParticipationList>, Status> {
        Ok(Response::new(pb::RouteParticipationList {
            items: self
                .domain
                .list_route_participations(request.into_inner())
                .await
                .map_err(domain_error)?,
        }))
    }

    async fn route_context(
        &self,
        request: Request<pb::RouteContextRequest>,
    ) -> Result<Response<pb::RouteParticipationContext>, Status> {
        Ok(Response::new(
            self.domain
                .route_context(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }

    async fn set_route_participation(
        &self,
        request: Request<pb::RouteParticipationRequest>,
    ) -> Result<Response<pb::RouteParticipationState>, Status> {
        Ok(Response::new(
            self.domain
                .set_route_participation(request.into_inner())
                .await
                .map_err(domain_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.grpc_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_server::BbsServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_server::BbsServer::with_interceptor(
            GrpcServer { domain },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(addr)
        .await
}

fn domain_error(error: crate::domain::BbsError) -> Status {
    match error {
        crate::domain::BbsError::Validation(message) => Status::invalid_argument(message),
        crate::domain::BbsError::Repository(crate::datasource::DaoError::BlockedRelationship) => {
            Status::failed_precondition(error.to_string())
        }
        crate::domain::BbsError::Repository(crate::datasource::DaoError::CachePeerRefresh) => {
            Status::unavailable(error.to_string())
        }
        crate::domain::BbsError::Repository(_) => Status::internal(error.to_string()),
    }
}
