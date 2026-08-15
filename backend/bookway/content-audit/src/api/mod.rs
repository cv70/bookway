mod grpc;

#[path = "pb/bookway.content.audit.rs"]
pub mod pb;

pub(crate) use bookway_api::{
    AuditDecisionDto, ContentAppealDto, ContentAppealPageDto, ContentAppealQueryRequest,
    ContentAuditRequest, ContentAuditResponse, ContentReportActionDto, ContentReportDto,
    ContentReportPageDto, ContentReportQueryRequest, CreateContentAppealRequest,
    CreateContentReportRequest, ReviewContentAppealRequest, ReviewContentReportRequest,
};
pub use grpc::serve;
