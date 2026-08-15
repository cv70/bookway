pub use bookway_api::{ApiError, ApiResponse, ErrorResponse, HealthResponse};

pub mod pb {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/pb/bookway.ad.main.rs"
    ));
}
