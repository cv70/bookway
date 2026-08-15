#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, content_audit_server::ContentAudit};
use crate::{
    api::{
        ContentAppealQueryRequest, ContentAuditRequest, ContentReportQueryRequest,
        CreateContentAppealRequest, CreateContentReportRequest, ReviewContentAppealRequest,
        ReviewContentReportRequest,
    },
    domain::{AuditError, Domain},
};
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

    async fn report(
        &self,
        request: Request<pb::ReportRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let request = request.into_inner();
        let payload: CreateContentReportRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .report(
                &request.reporter_id,
                &request.content_id,
                payload,
                (!request.idempotency_key.is_empty()).then_some(request.idempotency_key),
            )
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }

    async fn list_reports(
        &self,
        request: Request<pb::ListReportsRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let payload: ContentReportQueryRequest = from_json(&request.into_inner().request_json)?;
        let response = self
            .domain
            .list_reports(payload)
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }

    async fn appeal(
        &self,
        request: Request<pb::AppealRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let request = request.into_inner();
        let payload: CreateContentAppealRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .appeal(
                &request.appellant_id,
                &request.content_id,
                payload,
                (!request.idempotency_key.is_empty()).then_some(request.idempotency_key),
            )
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }

    async fn review_report(
        &self,
        request: Request<pb::ReviewReportRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let request = request.into_inner();
        let payload: ReviewContentReportRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .review_report(&request.reviewer_id, &request.report_id, payload)
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }

    async fn list_appeals(
        &self,
        request: Request<pb::ListAppealsRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let payload: ContentAppealQueryRequest = from_json(&request.into_inner().request_json)?;
        let response = self
            .domain
            .list_appeals(payload)
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }

    async fn review_appeal(
        &self,
        request: Request<pb::ReviewAppealRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        require_moderation_access(&request)?;
        let request = request.into_inner();
        let payload: ReviewContentAppealRequest = from_json(&request.request_json)?;
        let response = self
            .domain
            .review_appeal(&request.reviewer_id, &request.appeal_id, payload)
            .await
            .map_err(audit_status)?;
        json_response(&response)
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
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

fn require_moderation_access<T>(request: &Request<T>) -> Result<(), Status> {
    if !std::env::var("SERVICE_AUTH_REQUIRED").is_ok_and(|value| value == "true") {
        return Ok(());
    }
    let expected = std::env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
    let actual = request
        .metadata()
        .get("x-service-token")
        .and_then(|value| value.to_str().ok());
    if expected.is_empty() || actual != Some(expected.as_str()) {
        return Err(Status::unauthenticated("invalid service credentials"));
    }
    Ok(())
}

fn audit_status(error: AuditError) -> Status {
    match error {
        AuditError::Validation(message) => Status::invalid_argument(message),
        AuditError::Repository(crate::datasource::RepositoryError::ReportNotFound(id)) => {
            Status::not_found(format!("report {id} was not found"))
        }
        AuditError::Repository(crate::datasource::RepositoryError::AppealNotFound(id)) => {
            Status::not_found(format!("appeal {id} was not found"))
        }
        AuditError::Repository(crate::datasource::RepositoryError::ReportConflict) => {
            Status::aborted("report is already in a terminal state")
        }
        AuditError::Repository(crate::datasource::RepositoryError::AppealConflict) => {
            Status::aborted("appeal is already in a terminal state")
        }
        AuditError::Repository(crate::datasource::RepositoryError::InvalidReview(message)) => {
            Status::invalid_argument(message)
        }
        AuditError::Repository(crate::datasource::RepositoryError::InvalidAppealReview(
            message,
        )) => Status::invalid_argument(message),
        AuditError::Repository(error) => Status::internal(error.to_string()),
    }
}
