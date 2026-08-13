mod grpc;
#[path = "pb/bookway.recommend.rank.rs"]
pub mod pb;

pub use grpc::serve;
