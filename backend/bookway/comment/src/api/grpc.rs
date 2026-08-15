#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, comment_server::Comment};
use crate::domain::Domain;
use bookway_api::{CommentQueryRequest, CreateCommentRequest};
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
        let request = request.into_inner();
        let payload = if request.request_json.is_empty() {
            CommentQueryRequest::default()
        } else {
            from_json(&request.request_json)?
        };
        json_response(
            &self
                .domain
                .list(&request.post_id, payload)
                .await
                .map_err(internal_error)?,
        )
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
                .create_with_context(
                    &request.user_id,
                    &request.post_id,
                    payload,
                    request.idempotency_key,
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn delete(
        &self,
        request: Request<pb::DeleteRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        self.domain
            .delete(&request.user_id, &request.post_id, &request.comment_id)
            .await
            .map_err(internal_error)?;
        json_response(&())
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::comment_server::CommentServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::comment_server::CommentServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::CommentError) -> Status {
    let message = error.to_string();
    match error {
        crate::domain::CommentError::Validation(_) => Status::invalid_argument(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::ReplyDepthExceeded,
        ) => Status::invalid_argument(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::ParentNotFound(_),
        ) => Status::not_found(message),
        crate::domain::CommentError::Repository(crate::datasource::RepositoryError::NotFound(
            _,
        )) => Status::not_found(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::IdempotencyConflict,
        ) => Status::already_exists(message),
        crate::domain::CommentError::Repository(crate::datasource::RepositoryError::Database(
            _,
        )) => Status::internal(message),
        crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::InvalidModerationState(_),
        )
        | crate::domain::CommentError::Repository(
            crate::datasource::RepositoryError::InvalidReplyHierarchy,
        ) => Status::internal(message),
    }
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
