#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, content_audit_server::ContentAudit};
use crate::domain::{AuditError, Domain};
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
    ) -> Result<Response<pb::AuditResponse>, Status> {
        let response = self
            .domain
            .audit(request.into_inner())
            .await
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(response))
    }

    async fn report(
        &self,
        request: Request<pb::CreateReportRequest>,
    ) -> Result<Response<pb::ContentReport>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .report(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
    }

    async fn list_reports(
        &self,
        request: Request<pb::ListReportsRequest>,
    ) -> Result<Response<pb::ReportPage>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .list_reports(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
    }

    async fn appeal(
        &self,
        request: Request<pb::CreateAppealRequest>,
    ) -> Result<Response<pb::ContentAppeal>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .appeal(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
    }

    async fn review_report(
        &self,
        request: Request<pb::ReviewReportRequest>,
    ) -> Result<Response<pb::ContentReport>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .review_report(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
    }

    async fn list_appeals(
        &self,
        request: Request<pb::ListAppealsRequest>,
    ) -> Result<Response<pb::AppealPage>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .list_appeals(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
    }

    async fn review_appeal(
        &self,
        request: Request<pb::ReviewAppealRequest>,
    ) -> Result<Response<pb::ContentAppeal>, Status> {
        require_moderation_access(&request)?;
        let response = self
            .domain
            .review_appeal(request.into_inner())
            .await
            .map_err(audit_status)?;
        Ok(Response::new(response))
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
        AuditError::Repository(crate::datasource::DaoError::ReportNotFound(id)) => {
            Status::not_found(format!("report {id} was not found"))
        }
        AuditError::Repository(crate::datasource::DaoError::AppealNotFound(id)) => {
            Status::not_found(format!("appeal {id} was not found"))
        }
        AuditError::Repository(crate::datasource::DaoError::ReportConflict) => {
            Status::aborted("report is already in a terminal state")
        }
        AuditError::Repository(crate::datasource::DaoError::AppealConflict) => {
            Status::aborted("appeal is already in a terminal state")
        }
        AuditError::Repository(crate::datasource::DaoError::InvalidReview(message)) => {
            Status::invalid_argument(message)
        }
        AuditError::Repository(crate::datasource::DaoError::InvalidAppealReview(message)) => {
            Status::invalid_argument(message)
        }
        AuditError::Repository(error) => Status::internal(error.to_string()),
    }
}
