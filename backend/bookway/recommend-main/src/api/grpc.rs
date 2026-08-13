use super::pb::{self, recommend_main_server::RecommendMain};
use crate::{api::FeedQueryRequest, domain::Domain};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl RecommendMain for GrpcServer {
    async fn feed(
        &self,
        request: Request<pb::FeedRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let query: FeedQueryRequest = serde_json::from_str(&request.into_inner().request_json)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let response = self.domain.recommend(query).await;
        Ok(Response::new(pb::JsonResponse {
            response_json: serde_json::to_string(&response)
                .map_err(|error| Status::internal(error.to_string()))?,
        }))
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::recommend_main_server::RecommendMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::recommend_main_server::RecommendMainServer::new(
            GrpcServer {
                domain: domain.clone(),
            },
        ))
        .serve(domain.config.listen_addr)
        .await
}

fn _serialize_marker<T: Serialize>(_: &T) {}
