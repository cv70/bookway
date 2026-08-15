#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_server::Bbs};
use crate::domain::Domain;
use bookway_api::{FollowRequest, SetRouteParticipationRequest, SocialContextRequest};
use serde::Serialize;
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
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let response = self
            .domain
            .context(SocialContextRequest {
                user_id: Some(request.into_inner().user_id),
                post_ids: None,
            })
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }

    async fn visibility_context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let response = self
            .domain
            .visibility_context(&request.into_inner().user_id)
            .await
            .map_err(domain_error)?;
        json_response(&response)
    }

    async fn set_edge(
        &self,
        request: Request<pb::SetEdgeRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: FollowRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .set_edge(&request.user_id, &request.target_user_id, payload)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }

    async fn list_route_participations(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let response = self
            .domain
            .list_route_participations(&request.into_inner().user_id)
            .await
            .map_err(domain_error)?;
        json_response(&response)
    }

    async fn route_context(
        &self,
        request: Request<pb::RouteContextRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let response = self
            .domain
            .route_context(&request.user_id, request.route_ids)
            .await
            .map_err(domain_error)?;
        json_response(&response)
    }

    async fn set_route_participation(
        &self,
        request: Request<pb::RouteParticipationRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: SetRouteParticipationRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .set_route_participation(&request.user_id, &request.route_id, payload)
            .await
            .map_err(domain_error)?;
        json_response(&response)
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

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn domain_error(error: crate::domain::BbsError) -> Status {
    match error {
        crate::domain::BbsError::Validation(message) => Status::invalid_argument(message),
        crate::domain::BbsError::Repository(
            crate::datasource::RepositoryError::BlockedRelationship,
        ) => Status::failed_precondition(error.to_string()),
        crate::domain::BbsError::Repository(_) => Status::internal(error.to_string()),
    }
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
