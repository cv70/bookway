#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, search_main_server::SearchMain};
use crate::domain::Domain;
use bookway_bbs_search_api::pb as search_pb;
use std::time::Duration;
use tonic::{Request, Response, Status};

const SEARCH_REQUEST_BUDGET: Duration = Duration::from_millis(140);

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl SearchMain for GrpcServer {
    async fn search(
        &self,
        request: Request<search_pb::SearchRequest>,
    ) -> Result<Response<search_pb::SearchResponse>, Status> {
        let response = within_search_budget(self.domain.search(request.into_inner()))
            .await?
            .map_err(internal_error)?;
        Ok(Response::new(response))
    }

    async fn suggestions(
        &self,
        request: Request<search_pb::SuggestionsRequest>,
    ) -> Result<Response<search_pb::SuggestionsResponse>, Status> {
        let response = within_search_budget(self.domain.suggestions(request.into_inner()))
            .await?
            .map_err(internal_error)?;
        Ok(Response::new(response))
    }

    async fn validate_attributions(
        &self,
        request: Request<pb::ValidateSearchAttributionsRequest>,
    ) -> Result<Response<pb::ValidateSearchAttributionsResponse>, Status> {
        let response = self
            .domain
            .validate_attributions(request.into_inner())
            .await
            .map_err(|error| match error {
                crate::datasource::SearchExposureError::PositionOutOfRange => {
                    Status::invalid_argument(error.to_string())
                }
                crate::datasource::SearchExposureError::Database(_) => {
                    Status::unavailable(error.to_string())
                }
            })?;
        Ok(Response::new(response))
    }
}

async fn within_search_budget<T>(
    operation: impl std::future::Future<Output = T>,
) -> Result<T, Status> {
    tokio::time::timeout(SEARCH_REQUEST_BUDGET, operation)
        .await
        .map_err(|_| Status::deadline_exceeded("search request exceeded the 140ms budget"))
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::search_main_server::SearchMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::search_main_server::SearchMainServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.listen_addr)
        .await
}

fn internal_error(error: crate::domain::SearchMainError) -> Status {
    match error {
        crate::domain::SearchMainError::EmptyQuery
        | crate::domain::SearchMainError::QueryTooLong
        | crate::domain::SearchMainError::InvalidCursor(_) => {
            Status::invalid_argument(error.to_string())
        }
        crate::domain::SearchMainError::CursorExpired => {
            Status::failed_precondition(error.to_string())
        }
        crate::domain::SearchMainError::Session(error) => Status::unavailable(error.to_string()),
        crate::domain::SearchMainError::Upstream { code, message }
        | crate::domain::SearchMainError::ContentUpstream { code, message }
        | crate::domain::SearchMainError::ResourceUpstream { code, message } => {
            Status::new(code, message)
        }
        crate::domain::SearchMainError::InvalidContentSummary => {
            Status::unavailable(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::within_search_budget;

    #[tokio::test]
    async fn search_deadline_is_enforced_before_the_p99_limit() {
        let result = within_search_budget(async {
            std::future::pending::<()>().await;
        })
        .await;

        assert_eq!(
            result
                .expect_err("slow search operation must time out")
                .code(),
            tonic::Code::DeadlineExceeded
        );
    }
}
