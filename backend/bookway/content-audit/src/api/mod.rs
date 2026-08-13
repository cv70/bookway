mod grpc;

#[path = "pb/bookway.content.audit.rs"]
pub mod pb;

pub(crate) use bookway_api::{AuditDecisionDto, ContentAuditRequest, ContentAuditResponse};
pub(crate) use grpc::serve;
