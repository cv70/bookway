#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, bbs_search_server::BbsSearch};
use crate::domain::Domain;
use bookway_api::{SearchQueryRequest, SuggestionQueryRequest};
use serde::Serialize;
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
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let query: SearchQueryRequest = from_json(&request.into_inner().request_json)?;
        json_response(
            &self
                .domain
                .search
                .search(query)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn suggestions(
        &self,
        request: Request<pb::SuggestionsRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = suggestion_request(request.into_inner())?;
        json_response(
            &self
                .domain
                .search
                .suggestions(request)
                .await
                .map_err(internal_error)?,
        )
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

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn suggestion_request(request: pb::SuggestionsRequest) -> Result<SuggestionQueryRequest, Status> {
    if request.request_json.trim().is_empty() {
        return Ok(SuggestionQueryRequest {
            q: request.query,
            ..Default::default()
        });
    }
    from_json(&request.request_json)
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

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
