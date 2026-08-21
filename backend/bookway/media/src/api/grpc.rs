#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, media_server::Media};
use crate::domain::Domain;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Media for GrpcServer {
    async fn create_upload(
        &self,
        request: Request<pb::CreateUploadRequest>,
    ) -> Result<Response<pb::UploadResponse>, Status> {
        Ok(Response::new(
            self.domain
                .create_upload(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn complete_upload(
        &self,
        request: Request<pb::ResourceRequest>,
    ) -> Result<Response<pb::MediaResource>, Status> {
        Ok(Response::new(
            self.domain
                .complete_upload(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get(
        &self,
        request: Request<pb::ResourceRequest>,
    ) -> Result<Response<pb::MediaResource>, Status> {
        Ok(Response::new(
            self.domain
                .get(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }

    async fn get_owned_ready_batch(
        &self,
        request: Request<pb::OwnedReadyMediaRequest>,
    ) -> Result<Response<pb::OwnedReadyMediaResponse>, Status> {
        Ok(Response::new(
            self.domain
                .owned_ready_batch(request.into_inner())
                .await
                .map_err(internal_error)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::media_server::MediaServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::media_server::MediaServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config.grpc_addr)
        .await
}

fn internal_error(error: crate::domain::MediaError) -> Status {
    match error {
        crate::domain::MediaError::Validation(message) => Status::invalid_argument(message),
        crate::domain::MediaError::Forbidden => Status::permission_denied(error.to_string()),
        crate::domain::MediaError::Dao(crate::datasource::DaoError::NotFound) => {
            Status::not_found("media asset was not found or is not ready")
        }
        crate::domain::MediaError::Dao(crate::datasource::DaoError::Database(_))
        | crate::domain::MediaError::Object(_) => Status::unavailable(error.to_string()),
    }
}
