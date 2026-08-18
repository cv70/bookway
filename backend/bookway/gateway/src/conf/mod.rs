use std::{env, net::SocketAddr};

use axum::http::HeaderValue;
use bookway_runtime::RuntimeError;

const DEFAULT_CORS_ALLOWED_ORIGINS: &str =
    "http://127.0.0.1:8081,http://localhost:8081,http://127.0.0.1:19006,http://localhost:19006";

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) account_url: String,
    pub(crate) growth_url: String,
    pub(crate) knowledge_catalog_url: String,
    pub(crate) bbs_feed_url: String,
    pub(crate) bbs_link_url: String,
    pub(crate) search_main_url: String,
    pub(crate) bbs_url: String,
    pub(crate) bbs_creator_url: String,
    pub(crate) bbs_message_url: String,
    pub(crate) comment_url: String,
    pub(crate) interaction_status_url: String,
    pub(crate) user_event_url: String,
    pub(crate) media_url: String,
    pub(crate) content_audit_url: String,
    pub(crate) feedback_url: String,
    pub(crate) ad_center_url: String,
    pub(crate) ad_main_url: String,
    pub(crate) mall_url: String,
    pub(crate) mall_order_url: String,
    pub(crate) cors_allowed_origins: Vec<HeaderValue>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("GATEWAY_ADDR", "0.0.0.0:8080")?,
            account_url: grpc_url("ACCOUNT_GRPC_URL", "http://127.0.0.1:8094"),
            growth_url: grpc_url("GROWTH_GRPC_URL", "http://127.0.0.1:8081"),
            knowledge_catalog_url: grpc_url("KNOWLEDGE_CATALOG_GRPC_URL", "http://127.0.0.1:8105"),
            bbs_feed_url: grpc_url("BBS_FEED_GRPC_URL", "http://127.0.0.1:8088"),
            bbs_link_url: grpc_url("BBS_LINK_GRPC_URL", "http://127.0.0.1:18004"),
            search_main_url: grpc_url("SEARCH_MAIN_GRPC_URL", "http://127.0.0.1:8090"),
            bbs_url: grpc_url("BBS_GRPC_URL", "http://127.0.0.1:18002"),
            bbs_creator_url: grpc_url("BBS_CREATOR_GRPC_URL", "http://127.0.0.1:18105"),
            bbs_message_url: grpc_url("BBS_MESSAGE_GRPC_URL", "http://127.0.0.1:18106"),
            comment_url: grpc_url("COMMENT_GRPC_URL", "http://127.0.0.1:18006"),
            interaction_status_url: grpc_url(
                "INTERACTION_STATUS_GRPC_URL",
                "http://127.0.0.1:18007",
            ),
            user_event_url: grpc_url("USER_EVENT_GRPC_URL", "http://127.0.0.1:18089"),
            media_url: grpc_url("MEDIA_GRPC_URL", "http://127.0.0.1:18091"),
            content_audit_url: grpc_url("CONTENT_AUDIT_GRPC_URL", "http://127.0.0.1:8092"),
            feedback_url: grpc_url("FEEDBACK_GRPC_URL", "http://127.0.0.1:8104"),
            ad_center_url: grpc_url("AD_CENTER_GRPC_URL", "http://127.0.0.1:8097"),
            ad_main_url: grpc_url("AD_MAIN_GRPC_URL", "http://127.0.0.1:8100"),
            mall_url: grpc_url("MALL_GRPC_URL", "http://127.0.0.1:8101"),
            mall_order_url: grpc_url("MALL_ORDER_GRPC_URL", "http://127.0.0.1:8103"),
            cors_allowed_origins: cors_allowed_origins()?,
        })
    }
}

fn grpc_url(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn cors_allowed_origins() -> Result<Vec<HeaderValue>, RuntimeError> {
    let configured = env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| DEFAULT_CORS_ALLOWED_ORIGINS.to_string());
    parse_cors_allowed_origins(&configured)
}

fn parse_cors_allowed_origins(value: &str) -> Result<Vec<HeaderValue>, RuntimeError> {
    let origins = value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            let is_http_origin = origin.starts_with("http://") || origin.starts_with("https://");
            let origin_without_scheme = origin
                .split_once("://")
                .map(|(_, value)| value)
                .unwrap_or_default();
            if origin == "*"
                || !is_http_origin
                || origin_without_scheme.is_empty()
                || origin_without_scheme.contains(['/', '?', '#'])
            {
                return Err(RuntimeError::InvalidCorsOrigins {
                    value: value.to_string(),
                });
            }
            origin
                .parse::<HeaderValue>()
                .map_err(|_| RuntimeError::InvalidCorsOrigins {
                    value: value.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if origins.is_empty() {
        return Err(RuntimeError::InvalidCorsOrigins {
            value: value.to_string(),
        });
    }
    Ok(origins)
}

#[cfg(test)]
mod tests {
    use super::parse_cors_allowed_origins;

    #[test]
    fn accepts_explicit_http_origins() {
        let origins =
            parse_cors_allowed_origins("https://app.bookway.example, http://localhost:8081")
                .expect("origins should parse");

        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0], "https://app.bookway.example");
    }

    #[test]
    fn rejects_wildcards_and_non_origins() {
        assert!(parse_cors_allowed_origins("*").is_err());
        assert!(parse_cors_allowed_origins("https://app.bookway.example/path").is_err());
        assert!(parse_cors_allowed_origins("bookway://mobile").is_err());
    }
}
