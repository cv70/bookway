use super::pb::{self, bbs_feed_server::BbsFeed};
use crate::{api::FeedQueryRequest, domain::Domain};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsFeed for GrpcServer {
    async fn feed(
        &self,
        request: Request<pb::FeedRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request: FeedQueryRequest = serde_json::from_str(&request.into_inner().request_json)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let response = self
            .domain
            .feed(request)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(pb::JsonResponse {
            response_json: serde_json::to_string(&response)
                .map_err(|error| Status::internal(error.to_string()))?,
        }))
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

fn _serialize_marker<T: Serialize>(_: &T) {}
