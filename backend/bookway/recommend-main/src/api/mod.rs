mod grpc;

#[path = "pb/bookway.recommend.main.rs"]
pub mod pb;

pub use grpc::serve;

pub(crate) use bookway_api::{
    ContentStatusDto, FeedDto, FeedItemDto, FeedMetaDto, FeedQueryRequest, GrowthDomainDto,
    PostSummaryDto,
};
