use super::pb::{self, content_audit_server::ContentAudit};
use crate::{api::ContentAuditRequest, domain::Domain};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl ContentAudit for GrpcServer {
    async fn audit(
        &self,
        request: Request<pb::AuditRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let payload: ContentAuditRequest = from_json(&request.into_inner().request_json)?;
        let response = self
            .domain
            .audit(payload)
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        json_response(&response)
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::content_audit_server::ContentAuditServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::content_audit_server::ContentAuditServer::new(
            GrpcServer { domain },
        ))
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
