mod grpc;

#[path = "pb/bookway.bbs.feed.rs"]
pub mod pb;

pub(crate) use grpc::serve;

pub(crate) use bookway_api::{FeedDto, FeedQueryRequest};
