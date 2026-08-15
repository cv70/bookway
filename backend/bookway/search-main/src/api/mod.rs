pub(crate) mod grpc;
pub use bookway_search_main_api::pb;
pub(crate) use grpc::serve;

#[allow(unused_imports)]
pub(crate) use bookway_search_main_api::*;
