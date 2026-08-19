use super::pb::{self, recommend_main_server::RecommendMain};
use crate::domain::Domain;
use std::{future::Future, time::Duration};
use tonic::{Request, Response, Status};

// Leave a small transport margin under the product's 150ms P99 objective.
const FEED_REQUEST_BUDGET: Duration = Duration::from_millis(140);

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
        let response = within_feed_budget(self.domain.recommend(request.into_inner())).await?;
        Ok(Response::new(response))
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

async fn within_feed_budget<T>(operation: impl Future<Output = T>) -> Result<T, Status> {
    tokio::time::timeout(FEED_REQUEST_BUDGET, operation)
        .await
        .map_err(|_| Status::deadline_exceeded("feed request exceeded the 140ms budget"))
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

#[cfg(test)]
mod tests {
    use super::within_feed_budget;

    #[tokio::test]
    async fn feed_deadline_is_enforced_before_the_p99_limit() {
        let result = within_feed_budget(async {
            std::future::pending::<()>().await;
        })
        .await;

        assert_eq!(
            result
                .expect_err("slow feed operation must time out")
                .code(),
            tonic::Code::DeadlineExceeded
        );
    }
}
