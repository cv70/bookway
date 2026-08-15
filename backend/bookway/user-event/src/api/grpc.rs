#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, user_event_server::UserEvent};
use crate::{api::UserEventBatchRequest, domain::Domain};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl UserEvent for GrpcServer {
    async fn ingest(
        &self,
        request: Request<pb::IngestRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UserEventBatchRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .events
            .ingest(&request.user_id, payload)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::user_event_server::UserEventServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::user_event_server::UserEventServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.grpc_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
