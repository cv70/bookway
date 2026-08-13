mod grpc;

#[path = "pb/bookway.growth.rs"]
pub mod pb;

pub(crate) use grpc::serve;

pub(crate) use bookway_api::{
    ActionDto, ActionStateDto, CreateJourneyRequest, GrowthDomainDto, JourneyDto, JourneyStatusDto,
    TodayDto,
};
