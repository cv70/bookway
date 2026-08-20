#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, user_event_server::UserEvent};
use crate::domain::Domain;
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
    ) -> Result<Response<pb::IngestResponse>, Status> {
        let response = self
            .domain
            .events
            .ingest(request.into_inner())
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(response))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::user_event_server::UserEventServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::user_event_server::UserEventServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}
