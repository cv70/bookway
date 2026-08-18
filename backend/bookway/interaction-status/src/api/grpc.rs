#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, interaction_status_server::InteractionStatus};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl InteractionStatus for GrpcServer {
    async fn context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::ReactionContext>, Status> {
        Ok(Response::new(
            self.domain
                .context(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn set_reaction(
        &self,
        request: Request<pb::SetReactionRequest>,
    ) -> Result<Response<pb::Reaction>, Status> {
        Ok(Response::new(
            self.domain
                .set_reaction(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::interaction_status_server::InteractionStatusServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::interaction_status_server::InteractionStatusServer::new(
            GrpcServer {
                domain: domain.clone(),
            },
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn internal_error(error: crate::domain::InteractionStatusError) -> Status {
    match error {
        crate::domain::InteractionStatusError::Validation(message) => Status::invalid_argument(message),
        crate::domain::InteractionStatusError::Repository(error) => Status::internal(error.to_string()),
    }
}
