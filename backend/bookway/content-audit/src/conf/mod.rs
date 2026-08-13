use std::{env, net::SocketAddr};

use bookway_runtime::RuntimeError;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) listen_addr: SocketAddr,
    pub(crate) blocked: Vec<String>,
    pub(crate) reviewing: Vec<String>,
}
impl Config {
    pub(crate) fn from_env() -> Result<Self, RuntimeError> {
        Ok(Self {
            listen_addr: bookway_runtime::listen_addr("CONTENT_AUDIT_ADDR", "127.0.0.1:8092")?,
            blocked: terms("AUDIT_BLOCKED_TERMS", "自杀教程,毒品购买,仇恨暴力"),
            reviewing: terms("AUDIT_REVIEW_TERMS", "包治百病,快速减肥,广告合作,私下交易"),
        })
    }
}
fn terms(key: &str, defaults: &str) -> Vec<String> {
    env::var(key)
        .unwrap_or_else(|_| defaults.to_string())
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}
