use std::{env, net::SocketAddr};

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) grpc_addr: SocketAddr,
    pub(crate) s3_endpoint: String,
    pub(crate) s3_bucket: String,
    pub(crate) s3_region: String,
    pub(crate) s3_access_key: String,
    pub(crate) s3_secret_key: String,
    pub(crate) cdn_base: String,
    pub(crate) proxy_upload: bool,
}
impl Config {
    pub(crate) fn from_env() -> Result<Self, bookway_runtime::RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("MEDIA_ADDR", "127.0.0.1:8091")?,
            grpc_addr: bookway_runtime::listen_addr("MEDIA_GRPC_ADDR", "127.0.0.1:18091")?,
            s3_endpoint: env::var("S3_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string()),
            s3_bucket: env::var("S3_BUCKET").unwrap_or_else(|_| "bookway-media".to_string()),
            s3_region: env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            s3_access_key: env::var("S3_ACCESS_KEY")
                .unwrap_or_else(|_| "bookway-local".to_string()),
            s3_secret_key: env::var("S3_SECRET_KEY")
                .unwrap_or_else(|_| "bookway-local-only".to_string()),
            cdn_base: env::var("CDN_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:9000/bookway-media".to_string())
                .trim_end_matches('/')
                .to_string(),
            proxy_upload: env::var("MEDIA_PROXY_UPLOAD").is_ok_and(|value| value == "true"),
        })
    }
}
