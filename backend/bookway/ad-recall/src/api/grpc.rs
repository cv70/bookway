#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, ad_recall_server::AdRecall};
use crate::Domain;
use bookway_ad_center_api::pb as center;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl AdRecall for GrpcServer {
    async fn recall(
        &self,
        request: Request<pb::RecallRequest>,
    ) -> Result<Response<center::CampaignList>, Status> {
        let campaigns = self
            .domain
            .recall(request.into_inner())
            .await
            .map_err(|error| Status::unavailable(error.to_string()))?;
        Ok(Response::new(campaigns))
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::ad_recall_server::AdRecallServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::ad_recall_server::AdRecallServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}
