mod grpc;
pub use bookway_content_audit_api::pb;

pub use grpc::serve;

#[allow(unused_imports)]
pub(crate) use bookway_content_audit_api::*;
