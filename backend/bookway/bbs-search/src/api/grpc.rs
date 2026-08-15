#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_search_server::BbsSearch};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsSearch for GrpcServer {
    async fn search(
        &self,
        request: Request<pb::SearchRequest>,
    ) -> Result<Response<pb::SearchResponse>, Status> {
        let response = self
            .domain
            .search(request.into_inner())
            .await
            .map_err(internal_error)?;
        Ok(Response::new(response))
    }

    async fn suggestions(
        &self,
        request: Request<pb::SuggestionsRequest>,
    ) -> Result<Response<pb::SuggestionsResponse>, Status> {
        let response = self
            .domain
            .suggestions(request.into_inner())
            .await
            .map_err(internal_error)?;
        Ok(Response::new(response))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_search_server::BbsSearchServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_search_server::BbsSearchServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.listen_addr)
        .await
}

fn internal_error(error: crate::domain::SearchError) -> Status {
    match error {
        crate::domain::SearchError::Validation(message) => Status::invalid_argument(message),
        crate::domain::SearchError::CursorExpired => {
            Status::failed_precondition("搜索会话已过期，请重新搜索")
        }
        crate::domain::SearchError::Source(error) => Status::unavailable(error.to_string()),
    }
}
