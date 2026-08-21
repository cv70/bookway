#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, feedback_server::Feedback};
use crate::domain::{Domain, FeedbackError};
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Feedback for GrpcServer {
    async fn create_feedback(
        &self,
        request: Request<pb::CreateFeedbackRequest>,
    ) -> Result<Response<pb::FeedbackItem>, Status> {
        Ok(Response::new(
            self.domain
                .create(request.into_inner())
                .await
                .map_err(feedback_status)?,
        ))
    }

    async fn list_own_feedback(
        &self,
        request: Request<pb::ListOwnFeedbackRequest>,
    ) -> Result<Response<pb::FeedbackList>, Status> {
        Ok(Response::new(
            self.domain
                .list_own(request.into_inner())
                .await
                .map_err(feedback_status)?,
        ))
    }

    async fn list_feedback(
        &self,
        request: Request<pb::ListFeedbackRequest>,
    ) -> Result<Response<pb::FeedbackList>, Status> {
        Ok(Response::new(
            self.domain
                .list(request.into_inner())
                .await
                .map_err(feedback_status)?,
        ))
    }

    async fn review_feedback(
        &self,
        request: Request<pb::ReviewFeedbackRequest>,
    ) -> Result<Response<pb::FeedbackItem>, Status> {
        Ok(Response::new(
            self.domain
                .review(request.into_inner())
                .await
                .map_err(feedback_status)?,
        ))
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::feedback_server::FeedbackServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::feedback_server::FeedbackServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}

fn feedback_status(error: FeedbackError) -> Status {
    match error {
        FeedbackError::Validation(message) => Status::invalid_argument(message),
        FeedbackError::Dao(crate::datasource::DaoError::NotFound(id)) => {
            Status::not_found(format!("feedback {id} was not found"))
        }
        FeedbackError::Dao(error) => Status::internal(error.to_string()),
    }
}
