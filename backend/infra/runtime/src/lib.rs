use std::{env, net::SocketAddr};

use axum::{
    Router,
    extract::Request,
    http::{StatusCode, header::HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid listen address in {key}: {value}")]
    InvalidAddress { key: String, value: String },
    #[error("failed to bind service: {0}")]
    Bind(#[source] std::io::Error),
    #[error("service failed: {0}")]
    Serve(#[source] std::io::Error),
}

static REQUESTS_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUESTS_FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_SUM_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_100: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_300: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_500: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REDIS_MANAGER: tokio::sync::OnceCell<redis::aio::ConnectionManager> =
    tokio::sync::OnceCell::const_new();

pub fn init_tracing(service_name: &'static str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .try_init();
    info!(service = service_name, "telemetry initialized");
}

pub fn listen_addr(key: &str, default: &str) -> Result<SocketAddr, RuntimeError> {
    let value = env::var(key).unwrap_or_else(|_| default.to_string());
    value.parse().map_err(|_| RuntimeError::InvalidAddress {
        key: key.to_string(),
        value,
    })
}

/// Adds liveness, readiness and a small Prometheus-compatible text endpoint.
/// Each process keeps counters local; scrape aggregation belongs to Prometheus.
pub fn observability(router: Router, service: &'static str) -> Router {
    let request_id = axum::http::HeaderName::from_static("x-request-id");
    router
        .route("/metrics", get(move || metrics(service)))
        .route("/ready", get(move || ready(service)))
        .layer(axum::middleware::from_fn(record_request))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

async fn metrics(service: &'static str) -> impl IntoResponse {
    let total = REQUESTS_TOTAL.load(std::sync::atomic::Ordering::Relaxed);
    let failed = REQUESTS_FAILED.load(std::sync::atomic::Ordering::Relaxed);
    let duration_sum_ms = REQUEST_DURATION_SUM_MS.load(std::sync::atomic::Ordering::Relaxed);
    let le_100 = REQUEST_DURATION_LE_100.load(std::sync::atomic::Ordering::Relaxed);
    let le_300 = REQUEST_DURATION_LE_300.load(std::sync::atomic::Ordering::Relaxed);
    let le_500 = REQUEST_DURATION_LE_500.load(std::sync::atomic::Ordering::Relaxed);
    (
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        format!(
            "# TYPE bookway_http_requests_total counter\nbookway_http_requests_total{{service=\"{service}\"}} {total}\n# TYPE bookway_http_requests_failed_total counter\nbookway_http_requests_failed_total{{service=\"{service}\"}} {failed}\n# TYPE bookway_http_request_duration_seconds histogram\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.1\"}} {le_100}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.3\"}} {le_300}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.5\"}} {le_500}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"+Inf\"}} {total}\nbookway_http_request_duration_seconds_sum{{service=\"{service}\"}} {}\nbookway_http_request_duration_seconds_count{{service=\"{service}\"}} {total}\n",
            duration_sum_ms as f64 / 1000.0
        ),
    )
}

async fn ready(service: &'static str) -> impl IntoResponse {
    let dependency_keys: &[&str] = match service {
        "media" => &["DATABASE_URL", "S3_ENDPOINT"],
        "bbs-search" => &["OPENSEARCH_URL"],
        "feature-main" => &["DATABASE_URL", "REDIS_URL"],
        "bbs" | "bbs-link" | "comment" | "commonlikestatus" | "content-audit" | "growth"
        | "recommend-main" | "user-event" => &["DATABASE_URL"],
        _ => &[],
    };
    for key in dependency_keys {
        if let Ok(value) = env::var(key)
            && !value.is_empty()
            && !tcp_ready(&value).await
        {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("dependency unavailable: {key}\n"),
            );
        }
    }
    if service == "outbox-relay"
        && let Ok(brokers) = env::var("KAFKA_BROKERS")
    {
        for broker in brokers
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !tcp_ready(&format!("tcp://{broker}")).await {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "dependency unavailable: KAFKA_BROKERS\n".to_string(),
                );
            }
        }
    }
    (StatusCode::OK, "ready\n".to_string())
}

async fn tcp_ready(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let port = url.port().unwrap_or(match url.scheme() {
        "postgres" | "postgresql" => 5432,
        "redis" => 6379,
        "https" => 443,
        _ => 80,
    });
    tokio::time::timeout(
        std::time::Duration::from_millis(500),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

async fn record_request(request: Request, next: Next) -> Response {
    let started = std::time::Instant::now();
    REQUESTS_TOTAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let response = next.run(request).await;
    if response.status().is_server_error() {
        REQUESTS_FAILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    REQUEST_DURATION_SUM_MS.fetch_add(elapsed_ms, std::sync::atomic::Ordering::Relaxed);
    if elapsed_ms <= 100 {
        REQUEST_DURATION_LE_100.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if elapsed_ms <= 300 {
        REQUEST_DURATION_LE_300.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    if elapsed_ms <= 500 {
        REQUEST_DURATION_LE_500.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    response
}

/// Adds the configured service token to internal requests. Services only enforce
/// it when SERVICE_AUTH_REQUIRED=true, which keeps local memory-mode development simple.
pub fn http_client() -> reqwest::Client {
    let connect_timeout = env_duration_ms("HTTP_CONNECT_TIMEOUT_MS", 500);
    let request_timeout = env_duration_ms("HTTP_REQUEST_TIMEOUT_MS", 2_000);
    reqwest::Client::builder()
        .default_headers(service_headers())
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn env_duration_ms(key: &str, default: u64) -> std::time::Duration {
    std::time::Duration::from_millis(
        env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default),
    )
}

fn service_headers() -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    if let Ok(token) = env::var("SERVICE_AUTH_TOKEN")
        && let Ok(value) = HeaderValue::try_from(token)
    {
        headers.insert("x-service-token", value);
    }
    headers
}

async fn service_auth(service: &'static str, request: Request, next: Next) -> Response {
    let required = env::var("SERVICE_AUTH_REQUIRED").is_ok_and(|value| value == "true");
    let path = request.uri().path();
    let is_probe = matches!(path, "/health" | "/ready" | "/metrics");
    if required && service != "gateway" && !is_probe {
        let expected = env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
        let actual = request
            .headers()
            .get("x-service-token")
            .and_then(|v| v.to_str().ok());
        if expected.is_empty() || actual != Some(expected.as_str()) {
            return (StatusCode::UNAUTHORIZED, "invalid service credentials\n").into_response();
        }
    }
    if service == "gateway" {
        rate_limit(request, next).await
    } else {
        next.run(request).await
    }
}

#[derive(serde::Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

async fn auth_user(service: &'static str, mut request: Request, next: Next) -> Response {
    let required = env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true");
    if service != "gateway" || !required || !request.uri().path().starts_with("/v1/") {
        return next.run(request).await;
    }
    let Some(secret) = env::var("AUTH_JWT_SECRET")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "AUTH_JWT_SECRET is required\n",
        )
            .into_response();
    };
    let Some(token) = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return (StatusCode::UNAUTHORIZED, "bearer token required\n").into_response();
    };
    let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    let claims = match jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) {
        Ok(data) if !data.claims.sub.trim().is_empty() => data.claims,
        _ => return (StatusCode::UNAUTHORIZED, "invalid bearer token\n").into_response(),
    };
    let _expires_at = claims.exp;
    if let Ok(user_id) = HeaderValue::try_from(claims.sub) {
        request.headers_mut().insert("x-user-id", user_id);
    }
    next.run(request).await
}

async fn rate_limit(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if matches!(path.as_str(), "/health" | "/ready" | "/metrics") || env::var("REDIS_URL").is_err()
    {
        return next.run(request).await;
    }
    let limit = env::var("RATE_LIMIT_PER_MINUTE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(600);
    let connect_timeout = env_duration_ms("REDIS_CONNECT_TIMEOUT_MS", 1_000);
    let manager = tokio::time::timeout(
        connect_timeout,
        REDIS_MANAGER.get_or_try_init(|| async {
            let url = env::var("REDIS_URL").map_err(|error| {
                redis::RedisError::from((redis::ErrorKind::IoError, "REDIS_URL", error.to_string()))
            })?;
            let client = redis::Client::open(url)?;
            redis::aio::ConnectionManager::new(client).await
        }),
    )
    .await;
    let Ok(Ok(manager)) = manager else {
        tracing::warn!(
            timeout_ms = connect_timeout.as_millis(),
            "redis rate limiter unavailable; request allowed"
        );
        return next.run(request).await;
    };
    let identity = request
        .headers()
        .get("x-user-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("anonymous");
    let minute = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 60)
        .unwrap_or(0);
    let key = format!("bookway:rate:{}:{}:{}", identity, path, minute);
    let mut connection = manager.clone();
    let command_timeout = env_duration_ms("REDIS_COMMAND_TIMEOUT_MS", 100);
    let count = tokio::time::timeout(
        command_timeout,
        redis::cmd("INCR")
            .arg(&key)
            .query_async::<u64>(&mut connection),
    )
    .await;
    let count = match count {
        Ok(Ok(count)) => count,
        Ok(Err(error)) => {
            tracing::warn!(%error, "redis rate limiter command failed; request allowed");
            return next.run(request).await;
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = command_timeout.as_millis(),
                "redis rate limiter command timed out; request allowed"
            );
            return next.run(request).await;
        }
    };
    if count == 1 {
        let _ = tokio::time::timeout(
            command_timeout,
            redis::cmd("EXPIRE")
                .arg(&key)
                .arg(60)
                .query_async::<bool>(&mut connection),
        )
        .await;
    }
    if count > limit {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded\n").into_response();
    }
    next.run(request).await
}

pub async fn serve(
    service_name: &'static str,
    address: SocketAddr,
    app: Router,
) -> Result<(), RuntimeError> {
    let app = observability(app, service_name)
        .layer(axum::middleware::from_fn(move |request, next| {
            service_auth(service_name, request, next)
        }))
        .layer(axum::middleware::from_fn(move |request, next| {
            auth_user(service_name, request, next)
        }));
    let listener = TcpListener::bind(address)
        .await
        .map_err(RuntimeError::Bind)?;
    info!(service = service_name, %address, "service listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(RuntimeError::Serve)
}

async fn shutdown_signal() {
    let interrupt = async {
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::warn!("failed to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(%error, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}
