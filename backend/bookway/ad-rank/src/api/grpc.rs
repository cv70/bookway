#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, ad_rank_server::AdRank};
use crate::Domain;
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}
#[tonic::async_trait]
impl AdRank for GrpcServer {
    async fn rank(
        &self,
        request: Request<pb::RankRequest>,
    ) -> Result<Response<pb::RankResponse>, Status> {
        Ok(Response::new(self.domain.rank(request.into_inner()).await))
    }
}
pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::ad_rank_server::AdRankServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::ad_rank_server::AdRankServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}
