use super::pb::{self, recommend_main_server::RecommendMain};
use crate::domain::Domain;
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
    ) -> Result<Response<pb::FeedResponse>, Status> {
        Ok(Response::new(
            self.domain.recommend(request.into_inner()).await,
        ))
    }

    async fn validate_attributions(
        &self,
        request: Request<pb::ValidateAttributionsRequest>,
    ) -> Result<Response<pb::ValidateAttributionsResponse>, Status> {
        let response = self
            .domain
            .validate_attributions(request.into_inner())
            .await
            .map_err(|error| match error {
                crate::datasource::ExposureError::PositionOutOfRange => {
                    Status::invalid_argument(error.to_string())
                }
                crate::datasource::ExposureError::Database(_) => {
                    Status::unavailable(error.to_string())
                }
            })?;
        Ok(Response::new(response))
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::recommend_main_server::RecommendMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(
            pb::recommend_main_server::RecommendMainServer::with_interceptor(
                GrpcServer {
                    domain: domain.clone(),
                },
                bookway_runtime::grpc_service_auth_interceptor,
            ),
        )
        .serve(domain.config.listen_addr)
        .await
}
