pub(crate) mod grpc;
#[path = "pb/bookway.bbs.search.rs"]
pub mod pb;
pub(crate) use bookway_api::{
    SearchQueryRequest, SearchResponseDto, SuggestionQueryRequest, SuggestionResponseDto,
};
pub(crate) use grpc::serve;
