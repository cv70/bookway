#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, ad_main_server::AdMain};
use crate::Domain;
use bookway_ad_center_api::pb as center;
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}
#[tonic::async_trait]
impl AdMain for GrpcServer {
    async fn decide(
        &self,
        request: Request<pb::DecisionRequest>,
    ) -> Result<Response<pb::DecisionResponse>, Status> {
        Ok(Response::new(
            self.domain
                .decide(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }
    async fn report_event(
        &self,
        request: Request<center::RecordEventRequest>,
    ) -> Result<Response<center::EventReceipt>, Status> {
        Ok(Response::new(
            self.domain
                .report_event(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }
}
pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::ad_main_server::AdMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::ad_main_server::AdMainServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}
fn ad_error(error: crate::domain::AdMainError) -> Status {
    match error {
        crate::domain::AdMainError::Validation(message) => Status::invalid_argument(message),
        crate::domain::AdMainError::Upstream(message) => Status::unavailable(message),
    }
}
