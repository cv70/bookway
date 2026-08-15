mod http;
mod rest;

pub(crate) use bookway_gateway_api::{ApiResponse, ErrorResponse, HealthResponse};
pub(crate) use http::serve;
