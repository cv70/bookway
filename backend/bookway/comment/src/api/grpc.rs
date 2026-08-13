use super::pb::{self, comment_server::Comment};
use crate::domain::Domain;
use bookway_api::CreateCommentRequest;
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Comment for GrpcServer {
    async fn list(
        &self,
        request: Request<pb::ListRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let post_id = request.into_inner().post_id;
        json_response(&self.domain.list(&post_id).await.map_err(internal_error)?)
    }

    async fn create(
        &self,
        request: Request<pb::CreateRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateCommentRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create(&request.user_id, &request.post_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::comment_server::CommentServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::comment_server::CommentServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.grpc_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::CommentError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
