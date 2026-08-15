mod grpc;
mod http;
pub use bookway_api::{ApiResponse, ErrorResponse, HealthResponse};
pub use bookway_media_api::pb;

pub(crate) use grpc::serve as serve_grpc;
pub(crate) use http::serve as serve_http;
