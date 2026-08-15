use super::pb::{self, bbs_feed_server::BbsFeed};
use crate::domain::Domain;
use bookway_recommend_main_api::pb as recommend_main;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsFeed for GrpcServer {
    async fn feed(
        &self,
        request: Request<recommend_main::FeedRequest>,
    ) -> Result<Response<recommend_main::FeedResponse>, Status> {
        let response = self
            .domain
            .feed(request.into_inner())
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(response))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_feed_server::BbsFeedServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_feed_server::BbsFeedServer::new(GrpcServer {
            domain,
        }))
        .serve(addr)
        .await
}
