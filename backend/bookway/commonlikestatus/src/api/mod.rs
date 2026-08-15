mod grpc;
mod http;
pub use bookway_commonlikestatus_api::pb;

pub(crate) use grpc::serve as serve_grpc;
pub(crate) use http::serve as serve_http;

#[allow(unused_imports)]
pub(crate) use bookway_commonlikestatus_api::*;
