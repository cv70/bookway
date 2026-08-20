#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, feature_main_server::FeatureMain};
use crate::domain::Domain;
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
    ) -> Result<Response<pb::FeaturesResponse>, Status> {
        Ok(Response::new(
            self.domain.features(request.into_inner()).await,
        ))
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
        .add_service(
            pb::feature_main_server::FeatureMainServer::with_interceptor(
                GrpcServer { domain },
                bookway_runtime::grpc_service_auth_interceptor,
            ),
        )
        .serve(addr)
        .await
}
