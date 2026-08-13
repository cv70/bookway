use super::pb::{self, bbs_link_server::BbsLink};
use crate::domain::Domain;
use bookway_api::{ContentQueryRequest, CreateContentRequest, UpdateContentRequest};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl BbsLink for GrpcServer {
    async fn list(
        &self,
        request: Request<pb::ListRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request: ContentQueryRequest = from_json(&request.into_inner().request_json)?;
        json_response(&self.domain.list(request).await.map_err(internal_error)?)
    }

    async fn get(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let id = request.into_inner().id;
        json_response(&self.domain.get(&id).await.map_err(internal_error)?)
    }

    async fn get_public(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let id = request.into_inner().id;
        json_response(&self.domain.get_public(&id).await.map_err(internal_error)?)
    }

    async fn create(
        &self,
        request: Request<pb::CreateRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateContentRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create(
                    &request.user_id,
                    payload,
                    empty_to_none(request.idempotency_key),
                )
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update(
        &self,
        request: Request<pb::UpdateRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateContentRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update(&request.user_id, &request.id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn publish(
        &self,
        request: Request<pb::PublishRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .publish(&request.user_id, &request.id)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_link_server::BbsLinkServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_link_server::BbsLinkServer::new(GrpcServer {
            domain: domain.clone(),
        }))
        .serve(domain.config.grpc_addr)
        .await
}

fn empty_to_none(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::ContentError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
