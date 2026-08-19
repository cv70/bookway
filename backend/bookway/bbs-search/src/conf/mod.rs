use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) bbs_link_url: String,
    pub(crate) opensearch_url: Option<String>,
    pub(crate) opensearch_read_alias: String,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        let opensearch_url = env::var("OPENSEARCH_URL")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let opensearch_read_alias = read_alias(
            opensearch_url.as_deref(),
            non_empty_env("OPENSEARCH_READ_ALIAS"),
        )?;
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("BBS_SEARCH_ADDR", "127.0.0.1:8085")?,
            bbs_link_url: env::var("BBS_LINK_GRPC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18004".to_string()),
            opensearch_url,
            opensearch_read_alias,
        })
    }
}

fn read_alias(
    opensearch_url: Option<&str>,
    configured_alias: Option<String>,
) -> Result<String, RuntimeError> {
    match (opensearch_url, configured_alias) {
        (Some(_), Some(alias)) => Ok(alias),
        (Some(_), None) => Err(RuntimeError::InvalidSetting {
            key: "OPENSEARCH_READ_ALIAS".to_string(),
            value: "required when OPENSEARCH_URL is configured".to_string(),
        }),
        (None, _) => Ok("bookway-content".to_string()),
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::read_alias;

    #[test]
    fn configured_opensearch_requires_an_explicit_read_alias() {
        assert!(read_alias(Some("http://search"), None).is_err());
        assert_eq!(
            read_alias(Some("http://search"), Some("bookway-content".to_string()))
                .expect("configured alias should be accepted"),
            "bookway-content"
        );
    }
}
