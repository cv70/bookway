#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, account_server::Account};
use crate::{
    datasource::DaoError,
    domain::{AccountError, Domain},
};
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Account for GrpcServer {
    async fn profile(
        &self,
        request: Request<pb::ProfileRequest>,
    ) -> Result<Response<pb::AccountProfile>, Status> {
        let request = request.into_inner();
        Ok(Response::new(
            self.domain
                .profile(&request.user_id)
                .await
                .map_err(account_error)?,
        ))
    }

    async fn update_profile(
        &self,
        request: Request<pb::UpdateProfileRequest>,
    ) -> Result<Response<pb::AccountProfile>, Status> {
        Ok(Response::new(
            self.domain
                .update_profile(request.into_inner())
                .await
                .map_err(account_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::account_server::AccountServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::account_server::AccountServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.listen_addr)
        .await
}

fn account_error(error: AccountError) -> Status {
    let message = error.to_string();
    match error {
        AccountError::Validation(_) => Status::invalid_argument(message),
        AccountError::Dao(DaoError::NotFound(_)) => Status::not_found(message),
        AccountError::Dao(DaoError::Database(_)) => Status::internal(message),
    }
}
