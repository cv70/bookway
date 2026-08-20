use super::pb::{self, recommend_recall_server::RecommendRecall};
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
        .set_serving::<pb::recommend_recall_server::RecommendRecallServer<GrpcServer>>()
        .await;
    Server::builder()
        .add_service(health_service)
        .add_service(
            pb::recommend_recall_server::RecommendRecallServer::with_interceptor(
                GrpcServer::new(domain),
                bookway_runtime::grpc_service_auth_interceptor,
            ),
        )
        .serve(addr)
        .await
}

#[tonic::async_trait]
impl RecommendRecall for GrpcServer {
    async fn recall(
        &self,
        request: Request<pb::RecallRequest>,
    ) -> Result<Response<pb::RecallResponse>, Status> {
        Ok(Response::new(
            self.domain.recall(request.into_inner()).await,
        ))
    }
}
