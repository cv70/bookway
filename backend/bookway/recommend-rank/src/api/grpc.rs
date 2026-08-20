use super::pb::{self, recommend_rank_server::RecommendRank};
use crate::domain::Domain;
use std::net::SocketAddr;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub(crate) struct GrpcServer {
    domain: Domain,
}

impl GrpcServer {
    pub(crate) fn new(domain: Domain) -> Self {
        Self { domain }
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr: SocketAddr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::recommend_rank_server::RecommendRankServer<GrpcServer>>()
        .await;
    Server::builder()
        .add_service(health_service)
        .add_service(
            pb::recommend_rank_server::RecommendRankServer::with_interceptor(
                GrpcServer::new(domain),
                bookway_runtime::grpc_service_auth_interceptor,
            ),
        )
        .serve(addr)
        .await
}

#[tonic::async_trait]
impl RecommendRank for GrpcServer {
    async fn rank(
        &self,
        request: Request<pb::RankRequest>,
    ) -> Result<Response<pb::RankResponse>, Status> {
        Ok(Response::new(self.domain.rank(request.into_inner()).await))
    }
}
