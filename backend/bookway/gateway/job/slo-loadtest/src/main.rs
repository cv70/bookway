use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use thiserror::Error;
use tokio::task::JoinSet;

#[derive(Debug, Error)]
enum LoadTestError {
    #[error("invalid {key}: {value}")]
    InvalidSetting { key: &'static str, value: String },
    #[error("load test failed: {0}")]
    Failed(String),
}

#[derive(Clone, Debug)]
struct Config {
    gateway_url: String,
    user_id: String,
    bearer_token: Option<String>,
    search_query: String,
    requests_per_surface: usize,
    concurrency: usize,
    p99_budget_ms: u128,
    request_timeout: Duration,
    action_context: Option<ActionContext>,
}

#[derive(Clone, Debug)]
struct ActionContext {
    route_id: String,
    action_node_id: String,
    scene_equipment: String,
    placement: String,
}

impl Config {
    fn from_env() -> Result<Self, LoadTestError> {
        let gateway_url = env::var("GATEWAY_LOADTEST_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
        if !(gateway_url.starts_with("http://") || gateway_url.starts_with("https://")) {
            return Err(LoadTestError::InvalidSetting {
                key: "GATEWAY_LOADTEST_URL",
                value: gateway_url,
            });
        }
        let user_id = non_empty("GATEWAY_LOADTEST_USER_ID", "slo-loadtest-user")?;
        let bearer_token = env::var("GATEWAY_LOADTEST_BEARER_TOKEN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let search_query = non_empty("GATEWAY_LOADTEST_SEARCH_QUERY", "跑步装备")?;
        let action_context = action_context_from_env()?;
        Ok(Self {
            gateway_url: gateway_url.trim_end_matches('/').to_string(),
            user_id,
            bearer_token,
            search_query,
            requests_per_surface: env_number("GATEWAY_LOADTEST_REQUESTS", 1_000_usize)?
                .clamp(1, 1_000_000),
            concurrency: env_number("GATEWAY_LOADTEST_CONCURRENCY", 20_usize)?.clamp(1, 10_000),
            p99_budget_ms: env_number("GATEWAY_LOADTEST_P99_MS", 150_u128)?.clamp(1, 60_000),
            request_timeout: Duration::from_millis(
                env_number("GATEWAY_LOADTEST_REQUEST_TIMEOUT_MS", 500_u64)?.clamp(1, 60_000),
            ),
            action_context,
        })
    }
}

fn env_number<T>(key: &'static str, default: T) -> Result<T, LoadTestError>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| LoadTestError::InvalidSetting { key, value }),
        Err(_) => Ok(default),
    }
}

fn non_empty(key: &'static str, default: &str) -> Result<String, LoadTestError> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    if value.trim().is_empty() {
        return Err(LoadTestError::InvalidSetting { key, value });
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
enum Surface {
    Feed,
    Search,
}

impl Surface {
    fn name(self) -> &'static str {
        match self {
            Self::Feed => "feed",
            Self::Search => "search",
        }
    }

    fn endpoint(self, config: &Config) -> String {
        match self {
            Self::Feed => {
                let base = format!(
                    "{}/v1/feed?interests=learning&limit=10&surface=home",
                    config.gateway_url
                );
                append_action_context(base, self, config.action_context.as_ref())
            }
            Self::Search => {
                let base = format!(
                    "{}/v1/search?q={}&search_type=all&limit=10",
                    config.gateway_url,
                    percent_encode_query(&config.search_query)
                );
                append_action_context(base, self, config.action_context.as_ref())
            }
        }
    }
}

fn action_context_from_env() -> Result<Option<ActionContext>, LoadTestError> {
    let route_id = env::var("GATEWAY_LOADTEST_ROUTE_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let action_node_id = env::var("GATEWAY_LOADTEST_ACTION_NODE_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let scene_equipment = env::var("GATEWAY_LOADTEST_SCENE_EQUIPMENT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let configured = [
        route_id.is_some(),
        action_node_id.is_some(),
        scene_equipment.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if configured == 0 {
        return Ok(None);
    }
    if configured != 3 {
        return Err(LoadTestError::InvalidSetting {
            key: "GATEWAY_LOADTEST_ROUTE_ID/ACTION_NODE_ID/SCENE_EQUIPMENT",
            value: "all three action context fields are required together".to_string(),
        });
    }
    let placement = env::var("GATEWAY_LOADTEST_ACTION_PLACEMENT")
        .unwrap_or_else(|_| "action_node".to_string())
        .trim()
        .to_string();
    if placement.is_empty() {
        return Err(LoadTestError::InvalidSetting {
            key: "GATEWAY_LOADTEST_ACTION_PLACEMENT",
            value: placement,
        });
    }
    let route_id = route_id.ok_or_else(|| LoadTestError::InvalidSetting {
        key: "GATEWAY_LOADTEST_ROUTE_ID/ACTION_NODE_ID/SCENE_EQUIPMENT",
        value: "route id is required with action context".to_string(),
    })?;
    let action_node_id = action_node_id.ok_or_else(|| LoadTestError::InvalidSetting {
        key: "GATEWAY_LOADTEST_ROUTE_ID/ACTION_NODE_ID/SCENE_EQUIPMENT",
        value: "action node id is required with action context".to_string(),
    })?;
    let scene_equipment = scene_equipment.ok_or_else(|| LoadTestError::InvalidSetting {
        key: "GATEWAY_LOADTEST_ROUTE_ID/ACTION_NODE_ID/SCENE_EQUIPMENT",
        value: "scene equipment is required with action context".to_string(),
    })?;
    Ok(Some(ActionContext {
        route_id,
        action_node_id,
        scene_equipment,
        placement,
    }))
}

fn append_action_context(
    mut endpoint: String,
    surface: Surface,
    context: Option<&ActionContext>,
) -> String {
    let Some(context) = context else {
        return endpoint;
    };
    let params = [
        ("route_id", context.route_id.as_str()),
        ("action_node_id", context.action_node_id.as_str()),
        ("scene_equipment", context.scene_equipment.as_str()),
    ];
    for (key, value) in params {
        endpoint.push('&');
        endpoint.push_str(key);
        endpoint.push('=');
        endpoint.push_str(&percent_encode_query(value));
    }
    endpoint.push('&');
    endpoint.push_str(match surface {
        Surface::Feed => "placement",
        Surface::Search => "ad_placement",
    });
    endpoint.push('=');
    endpoint.push_str(&percent_encode_query(&context.placement));
    endpoint
}

fn percent_encode_query(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[derive(Clone, Debug)]
struct Sample {
    elapsed_ms: u128,
    success: bool,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SurfaceReport {
    surface: &'static str,
    requests: usize,
    succeeded: usize,
    failed: usize,
    p50_ms: Option<u128>,
    p95_ms: Option<u128>,
    p99_ms: Option<u128>,
    max_ms: Option<u128>,
    errors: Vec<String>,
}

impl SurfaceReport {
    fn passed(&self, p99_budget_ms: u128) -> bool {
        self.failed == 0 && self.p99_ms.is_some_and(|p99| p99 <= p99_budget_ms)
    }
}

#[derive(Debug, Serialize)]
struct LoadTestReport {
    p99_budget_ms: u128,
    requests_per_surface: usize,
    concurrency: usize,
    contextual_action_node: bool,
    surfaces: Vec<SurfaceReport>,
}

async fn run_surface(client: reqwest::Client, config: &Config, surface: Surface) -> SurfaceReport {
    let next_request = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let endpoint = surface.endpoint(config);
    let requests_per_surface = config.requests_per_surface;
    for _ in 0..config.concurrency.min(requests_per_surface) {
        let next_request = next_request.clone();
        let client = client.clone();
        let endpoint = endpoint.clone();
        let user_id = config.user_id.clone();
        let bearer_token = config.bearer_token.clone();
        tasks.spawn(async move {
            let mut samples = Vec::new();
            loop {
                let request_index = next_request.fetch_add(1, Ordering::Relaxed);
                if request_index >= requests_per_surface {
                    return samples;
                }
                let started = Instant::now();
                let mut request = client.get(&endpoint);
                if let Some(token) = bearer_token.as_deref() {
                    request = request.bearer_auth(token);
                } else {
                    request = request.header("x-user-id", &user_id);
                }
                let response = request.send().await;
                samples.push(match response {
                    Ok(response) if response.status().is_success() => Sample {
                        elapsed_ms: started.elapsed().as_millis(),
                        success: true,
                        error: None,
                    },
                    Ok(response) => Sample {
                        elapsed_ms: started.elapsed().as_millis(),
                        success: false,
                        error: Some(format!("HTTP {}", response.status())),
                    },
                    Err(error) => Sample {
                        elapsed_ms: started.elapsed().as_millis(),
                        success: false,
                        error: Some(error.to_string()),
                    },
                });
            }
        });
    }

    let mut samples = Vec::with_capacity(config.requests_per_surface);
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(mut worker_samples) => samples.append(&mut worker_samples),
            Err(error) => samples.push(Sample {
                elapsed_ms: 0,
                success: false,
                error: Some(format!("load task failed: {error}")),
            }),
        }
    }
    report(surface, samples)
}

fn report(surface: Surface, samples: Vec<Sample>) -> SurfaceReport {
    let mut successes = samples
        .iter()
        .filter(|sample| sample.success)
        .map(|sample| sample.elapsed_ms)
        .collect::<Vec<_>>();
    successes.sort_unstable();
    let errors = samples
        .iter()
        .filter_map(|sample| sample.error.clone())
        .take(10)
        .collect::<Vec<_>>();
    let succeeded = successes.len();
    SurfaceReport {
        surface: surface.name(),
        requests: samples.len(),
        succeeded,
        failed: samples.len().saturating_sub(succeeded),
        p50_ms: percentile(&successes, 50),
        p95_ms: percentile(&successes, 95),
        p99_ms: percentile(&successes, 99),
        max_ms: successes.last().copied(),
        errors,
    }
}

fn percentile(samples: &[u128], percentile: usize) -> Option<u128> {
    if samples.is_empty() || percentile == 0 || percentile > 100 {
        return None;
    }
    let rank = (samples.len() * percentile).div_ceil(100).saturating_sub(1);
    samples.get(rank).copied()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("gateway-slo-loadtest");
    let config = Config::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(config.request_timeout)
        .build()?;
    let mut reports = Vec::new();
    for surface in [Surface::Feed, Surface::Search] {
        reports.push(run_surface(client.clone(), &config, surface).await);
    }
    let report = LoadTestReport {
        p99_budget_ms: config.p99_budget_ms,
        requests_per_surface: config.requests_per_surface,
        concurrency: config.concurrency,
        contextual_action_node: config.action_context.is_some(),
        surfaces: reports,
    };
    println!("{}", serde_json::to_string(&report)?);
    if report
        .surfaces
        .iter()
        .all(|surface| surface.passed(config.p99_budget_ms))
    {
        return Ok(());
    }
    Err(Box::new(LoadTestError::Failed(format!(
        "Feed/Search must return only 2xx responses with P99 <= {}ms",
        config.p99_budget_ms
    ))) as Box<dyn std::error::Error>)
}

#[cfg(test)]
mod tests {
    use super::{
        ActionContext, Sample, Surface, append_action_context, percent_encode_query, percentile,
        report,
    };

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 99), Some(50));
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 50), Some(30));
    }

    #[test]
    fn any_failed_request_fails_the_surface_even_within_budget() {
        let result = report(
            Surface::Feed,
            vec![
                Sample {
                    elapsed_ms: 50,
                    success: true,
                    error: None,
                },
                Sample {
                    elapsed_ms: 60,
                    success: false,
                    error: Some("HTTP 503".to_string()),
                },
            ],
        );
        assert_eq!(result.p99_ms, Some(50));
        assert!(!result.passed(150));
    }

    #[test]
    fn p99_over_budget_fails_an_otherwise_successful_surface() {
        let result = report(
            Surface::Search,
            vec![
                Sample {
                    elapsed_ms: 100,
                    success: true,
                    error: None,
                },
                Sample {
                    elapsed_ms: 200,
                    success: true,
                    error: None,
                },
            ],
        );
        assert_eq!(result.p99_ms, Some(200));
        assert!(!result.passed(150));
    }

    #[test]
    fn search_query_is_percent_encoded_before_building_the_gateway_url() {
        assert_eq!(
            percent_encode_query("跑步 装备"),
            "%E8%B7%91%E6%AD%A5%20%E8%A3%85%E5%A4%87"
        );
    }

    #[test]
    fn contextual_loadtest_urls_carry_the_complete_action_context() {
        let context = ActionContext {
            route_id: "route/one".to_string(),
            action_node_id: "node-1".to_string(),
            scene_equipment: "trail shoes".to_string(),
            placement: "action node".to_string(),
        };
        let endpoint = append_action_context(
            "http://127.0.0.1:8080/v1/search?q=walk".to_string(),
            Surface::Search,
            Some(&context),
        );
        assert!(endpoint.contains("route_id=route%2Fone"));
        assert!(endpoint.contains("action_node_id=node-1"));
        assert!(endpoint.contains("scene_equipment=trail%20shoes"));
        assert!(endpoint.contains("ad_placement=action%20node"));
    }
}
