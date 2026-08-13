mod grpc;
#[path = "pb/bookway.recommend.recall.rs"]
pub mod pb;

pub use grpc::serve;
