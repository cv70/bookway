mod grpc;
mod http;

#[path = "pb/bookway.bbs.rs"]
pub mod pb;

pub(crate) use grpc::serve as serve_grpc;
pub(crate) use http::serve as serve_http;

pub(crate) use bookway_api::{
    RouteParticipationContextDto, RouteParticipationDto, RouteParticipationStateDto,
    SocialContextDto, SocialEdgeTypeDto, SocialVisibilityDto,
};
