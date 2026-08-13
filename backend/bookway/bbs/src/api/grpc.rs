use super::pb::{self, bbs_server::Bbs};
use crate::domain::Domain;
use bookway_api::{FollowRequest, SocialContextRequest};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Bbs for GrpcServer {
    async fn context(
        &self,
        request: Request<pb::ContextRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let response = self
            .domain
            .context(SocialContextRequest {
                user_id: Some(request.into_inner().user_id),
                post_ids: None,
            })
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }

    async fn set_edge(
        &self,
        request: Request<pb::SetEdgeRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: FollowRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .set_edge(&request.user_id, &request.target_user_id, payload)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.grpc_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::bbs_server::BbsServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::bbs_server::BbsServer::new(GrpcServer { domain }))
        .serve(addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
