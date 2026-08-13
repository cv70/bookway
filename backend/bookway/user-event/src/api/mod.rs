mod http;
pub(crate) use http::serve as serve_http;

pub(crate) use bookway_api::{UserEventBatchRequest, UserEventDto, UserEventIngestResponse};
pub(crate) mod grpc;
#[path = "pb/bookway.user.event.rs"]
pub mod pb;
pub(crate) use grpc::serve as serve_grpc;
