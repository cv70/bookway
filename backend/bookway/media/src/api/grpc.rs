use super::pb::{self, media_server::Media};
use crate::{api::UploadRequest, domain::Domain};
use serde::Serialize;
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
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UploadRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_upload(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn complete_upload(
        &self,
        request: Request<pb::ResourceRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .complete_upload(&request.user_id, &request.id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn get(
        &self,
        request: Request<pb::ResourceRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .get(&request.user_id, &request.id)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::media_server::MediaServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::media_server::MediaServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.grpc_addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::MediaError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
