use std::env;

use serde_json::{Value, json};
use thiserror::Error;

#[derive(Debug, Error)]
enum SwitchError {
    #[error("{key} is required")]
    MissingEnvironment { key: &'static str },
    #[error("invalid {key}: {value}")]
    InvalidEnvironment { key: &'static str, value: String },
    #[error("OPENSEARCH_READ_ALIAS and OPENSEARCH_WRITE_INDEX must differ")]
    SameAliasAndTarget,
    #[error("OpenSearch request failed: {0}")]
    Request(String),
    #[error("OpenSearch target index is missing: {0}")]
    TargetMissing(String),
    #[error("OpenSearch target must resolve to exactly one concrete index: {0}")]
    TargetNotConcrete(String),
    #[error("OpenSearch target validation failed: {0}")]
    TargetValidation(String),
    #[error("OpenSearch alias membership response is invalid: {0}")]
    InvalidMembership(String),
    #[error("OpenSearch alias switch was not acknowledged")]
    NotAcknowledged,
}

#[derive(Debug)]
struct Config {
    base_url: String,
    read_alias: String,
    target_index: String,
}

impl Config {
    fn from_env() -> Result<Self, SwitchError> {
        let base_url = required_env("OPENSEARCH_URL")?;
        let read_alias = required_env("OPENSEARCH_READ_ALIAS")?;
        let target_index = required_env("OPENSEARCH_WRITE_INDEX")?;
        validate_resource_name("OPENSEARCH_READ_ALIAS", &read_alias)?;
        validate_resource_name("OPENSEARCH_WRITE_INDEX", &target_index)?;
        if read_alias == target_index {
            return Err(SwitchError::SameAliasAndTarget);
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            read_alias,
            target_index,
        })
    }
}

fn required_env(key: &'static str) -> Result<String, SwitchError> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or(SwitchError::MissingEnvironment { key })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-index-alias-switch");
    let config = Config::from_env()?;
    let client = bookway_runtime::http_client();

    let document_count = validate_target(&client, &config).await?;
    let old_indices = alias_memberships(&client, &config).await?;
    let actions = build_alias_actions(&config.read_alias, &config.target_index, &old_indices);
    switch_alias(&client, &config.base_url, actions).await?;

    tracing::info!(
        read_alias = %config.read_alias,
        target_index = %config.target_index,
        replaced_indices = old_indices.len(),
        document_count,
        "OpenSearch read alias switched atomically"
    );
    println!(
        "{}",
        json!({
            "status": "switched",
            "read_alias": config.read_alias,
            "target_index": config.target_index,
            "replaced_indices": old_indices,
            "document_count": document_count,
        })
    );
    Ok(())
}

async fn validate_target(client: &reqwest::Client, config: &Config) -> Result<u64, SwitchError> {
    let target_url = resource_url(&config.base_url, &[&config.target_index])?;
    let exists = client
        .head(target_url)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if exists.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(SwitchError::TargetMissing(config.target_index.clone()));
    }
    if !exists.status().is_success() {
        return Err(SwitchError::TargetValidation(format!(
            "index existence check returned {}",
            exists.status()
        )));
    }

    let resolved = client
        .get(resource_url(
            &config.base_url,
            &["_resolve", "index", &config.target_index],
        )?)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if !resolved.status().is_success() {
        return Err(SwitchError::TargetValidation(format!(
            "index resolution returned {}",
            resolved.status()
        )));
    }
    let resolved = resolved
        .json::<Value>()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if !resolves_to_concrete_index(&resolved, &config.target_index) {
        return Err(SwitchError::TargetNotConcrete(config.target_index.clone()));
    }

    refresh_target(client, config).await?;

    let count = client
        .get(resource_url(
            &config.base_url,
            &[&config.target_index, "_count"],
        )?)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if !count.status().is_success() {
        return Err(SwitchError::TargetValidation(format!(
            "index count check returned {}",
            count.status()
        )));
    }
    count
        .json::<Value>()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            SwitchError::TargetValidation("count response has no numeric count".to_string())
        })
}

async fn refresh_target(client: &reqwest::Client, config: &Config) -> Result<(), SwitchError> {
    let response = client
        .post(resource_url(
            &config.base_url,
            &[&config.target_index, "_refresh"],
        )?)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(SwitchError::TargetValidation(format!(
            "index refresh returned {}",
            response.status()
        )))
    }
}

async fn alias_memberships(
    client: &reqwest::Client,
    config: &Config,
) -> Result<Vec<String>, SwitchError> {
    let response = client
        .get(resource_url(
            &config.base_url,
            &["_alias", &config.read_alias],
        )?)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !response.status().is_success() {
        return Err(SwitchError::InvalidMembership(format!(
            "alias lookup returned {}",
            response.status()
        )));
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    parse_alias_memberships(&payload, &config.read_alias)
}

fn parse_alias_memberships(payload: &Value, read_alias: &str) -> Result<Vec<String>, SwitchError> {
    let entries = payload.as_object().ok_or_else(|| {
        SwitchError::InvalidMembership("response must be an index object".to_string())
    })?;
    let mut indices = Vec::with_capacity(entries.len());
    for (index, metadata) in entries {
        validate_resource_name("alias member index", index)
            .map_err(|_| SwitchError::InvalidMembership(format!("unsafe member index: {index}")))?;
        if metadata
            .get("aliases")
            .and_then(Value::as_object)
            .and_then(|aliases| aliases.get(read_alias))
            .is_none()
        {
            return Err(SwitchError::InvalidMembership(format!(
                "member {index} does not contain alias {read_alias}"
            )));
        }
        indices.push(index.clone());
    }
    indices.sort();
    Ok(indices)
}

fn resolves_to_concrete_index(payload: &Value, target_index: &str) -> bool {
    let has_exact_index = payload
        .get("indices")
        .and_then(Value::as_array)
        .is_some_and(|indices| {
            indices
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    let has_same_named_alias = payload
        .get("aliases")
        .and_then(Value::as_array)
        .is_some_and(|aliases| {
            aliases
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    let has_same_named_data_stream = payload
        .get("data_streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|entry| entry.get("name").and_then(Value::as_str) == Some(target_index))
        });
    has_exact_index && !has_same_named_alias && !has_same_named_data_stream
}

fn build_alias_actions(read_alias: &str, target_index: &str, old_indices: &[String]) -> Value {
    let mut actions = old_indices
        .iter()
        .map(|index| json!({ "remove": { "index": index, "alias": read_alias } }))
        .collect::<Vec<_>>();
    actions.push(json!({ "add": { "index": target_index, "alias": read_alias } }));
    json!({ "actions": actions })
}

async fn switch_alias(
    client: &reqwest::Client,
    base_url: &str,
    actions: Value,
) -> Result<(), SwitchError> {
    let response = client
        .post(resource_url(base_url, &["_aliases"])?)
        .json(&actions)
        .send()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if !response.status().is_success() {
        return Err(SwitchError::Request(format!(
            "alias switch returned {}",
            response.status()
        )));
    }
    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| SwitchError::Request(error.to_string()))?;
    if payload.get("acknowledged").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(SwitchError::NotAcknowledged)
    }
}

fn validate_resource_name(key: &'static str, value: &str) -> Result<(), SwitchError> {
    let bytes = value.as_bytes();
    let valid_start = matches!(bytes.first(), Some(b'a'..=b'z' | b'0'..=b'9'));
    let valid_characters = bytes.iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(*byte, b'-' | b'_' | b'.')
    });
    if value.len() > 255
        || !valid_start
        || !valid_characters
        || matches!(value, "." | "..")
        || value.contains("..")
    {
        return Err(SwitchError::InvalidEnvironment {
            key,
            value: value.to_string(),
        });
    }
    Ok(())
}

fn resource_url(base_url: &str, path: &[&str]) -> Result<reqwest::Url, SwitchError> {
    let mut url =
        reqwest::Url::parse(base_url).map_err(|error| SwitchError::InvalidEnvironment {
            key: "OPENSEARCH_URL",
            value: error.to_string(),
        })?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| SwitchError::InvalidEnvironment {
            key: "OPENSEARCH_URL",
            value: "cannot be used as a base URL".to_string(),
        })?;
    segments.pop_if_empty();
    for segment in path {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_alias_actions, parse_alias_memberships, resolves_to_concrete_index, resource_url,
        validate_resource_name,
    };

    #[test]
    fn aliases_and_indices_have_strict_names() {
        assert!(validate_resource_name("key", "bookway-content-v2").is_ok());
        for invalid in ["Bookway-v2", ".system", "bookway/*", "bookway v2", ".."] {
            assert!(
                validate_resource_name("key", invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    #[test]
    fn alias_switch_removes_every_old_target_before_adding_new_target() {
        let actions = build_alias_actions(
            "bookway-content",
            "bookway-content-v2",
            &[
                "bookway-content-v1".to_string(),
                "bookway-content-v0".to_string(),
            ],
        );
        assert_eq!(
            actions,
            json!({
                "actions": [
                    { "remove": { "index": "bookway-content-v1", "alias": "bookway-content" } },
                    { "remove": { "index": "bookway-content-v0", "alias": "bookway-content" } },
                    { "add": { "index": "bookway-content-v2", "alias": "bookway-content" } }
                ]
            })
        );
    }

    #[test]
    fn target_must_resolve_as_a_concrete_index() {
        assert!(resolves_to_concrete_index(
            &json!({ "indices": [{ "name": "bookway-content-v2" }], "aliases": [], "data_streams": [] }),
            "bookway-content-v2"
        ));
        assert!(!resolves_to_concrete_index(
            &json!({ "indices": [], "aliases": [{ "name": "bookway-content", "indices": ["bookway-content-v2"] }], "data_streams": [] }),
            "bookway-content"
        ));
    }

    #[test]
    fn alias_membership_must_name_the_requested_alias() {
        let members = parse_alias_memberships(
            &json!({
                "bookway-content-v1": { "aliases": { "bookway-content": {} } },
                "bookway-content-v0": { "aliases": { "bookway-content": {} } }
            }),
            "bookway-content",
        )
        .expect("valid alias membership");
        assert_eq!(members, ["bookway-content-v0", "bookway-content-v1"]);
        assert!(
            parse_alias_memberships(
                &json!({ "bookway-content-v1": { "aliases": {} } }),
                "bookway-content"
            )
            .is_err()
        );
    }

    #[test]
    fn path_segments_escape_resource_names() {
        let url =
            resource_url("https://search.example/api", &["_alias", "a/b"]).expect("valid URL");
        assert_eq!(url.as_str(), "https://search.example/api/_alias/a%2Fb");
    }
}
