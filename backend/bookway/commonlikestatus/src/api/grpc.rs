#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, common_like_status_server::CommonLikeStatus};
use crate::domain::Domain;
use bookway_api::{ReactionContextRequest, ReactionRequest};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl CommonLikeStatus for GrpcServer {
    async fn context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let payload: ReactionContextRequest = from_json(&request.into_inner().request_json)?;
        json_response(&self.domain.context(payload).await.map_err(internal_error)?)
    }

    async fn set_reaction(
        &self,
        request: Request<pb::SetReactionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: ReactionRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .set_reaction(&request.user_id, &request.post_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::common_like_status_server::CommonLikeStatusServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::common_like_status_server::CommonLikeStatusServer::new(
            GrpcServer {
                domain: domain.clone(),
            },
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::LikeStatusError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
