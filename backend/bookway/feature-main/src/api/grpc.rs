#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, feature_main_server::FeatureMain};
use crate::{api::FeatureRequest, domain::Domain};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl FeatureMain for GrpcServer {
    async fn features(
        &self,
        request: Request<pb::FeaturesRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let response = self
            .domain
            .features(FeatureRequest {
                user_id: request.user_id,
                content_ids: request.content_ids,
            })
            .await;
        json_response(&response)
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::feature_main_server::FeatureMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::feature_main_server::FeatureMainServer::new(
            GrpcServer { domain },
        ))
        .serve(addr)
        .await
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
