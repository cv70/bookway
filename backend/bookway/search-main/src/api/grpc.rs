use super::pb::{self, search_main_server::SearchMain};
use crate::domain::Domain;
use bookway_api::SearchQueryRequest;
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl SearchMain for GrpcServer {
    async fn search(
        &self,
        request: Request<pb::SearchRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let query: SearchQueryRequest = from_json(&request.into_inner().request_json)?;
        json_response(
            &self
                .domain
                .service
                .search(query)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn suggestions(
        &self,
        request: Request<pb::SuggestionsRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let query = request.into_inner().query;
        json_response(
            &self
                .domain
                .service
                .suggestions(&query)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::search_main_server::SearchMainServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::search_main_server::SearchMainServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.listen_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::SearchMainError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
