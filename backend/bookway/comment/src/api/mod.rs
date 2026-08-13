mod grpc;
mod http;

#[path = "pb/bookway.comment.rs"]
pub mod pb;

pub(crate) use grpc::serve as serve_grpc;
pub(crate) use http::serve as serve_http;

pub(crate) use bookway_api::CommentDto;
