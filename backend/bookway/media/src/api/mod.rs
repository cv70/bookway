mod grpc;
mod http;

#[path = "pb/bookway.media.rs"]
pub mod pb;

pub(crate) use grpc::serve as serve_grpc;
pub(crate) use http::serve as serve_http;

pub(crate) use bookway_api::{
    MediaDto as MediaResponse, MediaUploadRequest as UploadRequest,
    MediaUploadResponse as UploadResponse,
};
