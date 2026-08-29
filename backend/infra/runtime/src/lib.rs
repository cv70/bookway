use std::{env, net::SocketAddr};

use axum::{
    Router,
    extract::Request,
    http::{Method, StatusCode, header::HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

pub mod grpc_client;
pub use grpc_client::{CircuitBreaker, ConnectFailure, grpc_channel};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("invalid listen address in {key}: {value}")]
    InvalidAddress { key: String, value: String },
    #[error("invalid CORS allowed origins: {value}")]
    InvalidCorsOrigins { value: String },
    #[error("invalid setting in {key}: {value}")]
    InvalidSetting { key: String, value: String },
    #[error("failed to bind service: {0}")]
    Bind(#[source] std::io::Error),
    #[error("service failed: {0}")]
    Serve(#[source] std::io::Error),
}

#[derive(Debug, Error)]
pub enum GrpcServiceAuthError {
    #[error("SERVICE_AUTH_TOKEN is required when SERVICE_AUTH_REQUIRED=true")]
    MissingToken,
    #[error("SERVICE_AUTH_TOKEN is not valid gRPC metadata")]
    InvalidToken,
}

static REQUESTS_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUESTS_FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_SUM_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_100: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_150: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_300: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REQUEST_DURATION_LE_500: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static REDIS_MANAGER: tokio::sync::OnceCell<redis::aio::ConnectionManager> =
    tokio::sync::OnceCell::const_new();
const RATE_LIMIT_LUA: &str = r#"
local count = redis.call('INCR', KEYS[1])
local ttl = redis.call('TTL', KEYS[1])
if count == 1 or ttl < 0 then
  redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return count
"#;

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
    let le_150 = REQUEST_DURATION_LE_150.load(std::sync::atomic::Ordering::Relaxed);
    let le_300 = REQUEST_DURATION_LE_300.load(std::sync::atomic::Ordering::Relaxed);
    let le_500 = REQUEST_DURATION_LE_500.load(std::sync::atomic::Ordering::Relaxed);
    (
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        format!(
            "# TYPE bookway_http_requests_total counter\nbookway_http_requests_total{{service=\"{service}\"}} {total}\n# TYPE bookway_http_requests_failed_total counter\nbookway_http_requests_failed_total{{service=\"{service}\"}} {failed}\n# TYPE bookway_http_request_duration_seconds histogram\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.1\"}} {le_100}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.15\"}} {le_150}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.3\"}} {le_300}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"0.5\"}} {le_500}\nbookway_http_request_duration_seconds_bucket{{service=\"{service}\",le=\"+Inf\"}} {total}\nbookway_http_request_duration_seconds_sum{{service=\"{service}\"}} {}\nbookway_http_request_duration_seconds_count{{service=\"{service}\"}} {total}\n",
            duration_sum_ms as f64 / 1000.0
        ),
    )
}

async fn ready(service: &'static str) -> impl IntoResponse {
    let dependency_keys: &[&str] = match service {
        "media" => &["DATABASE_URL", "S3_ENDPOINT"],
        "bbs-search" => &["OPENSEARCH_URL"],
        "feature-main" => &["DATABASE_URL", "REDIS_URL"],
        "bbs" | "bbs-link" | "comment" | "interaction-status" | "content-audit" | "gateway"
        | "growth" | "recommend-main" | "user-event" => &["DATABASE_URL"],
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
    if elapsed_ms <= 150 {
        REQUEST_DURATION_LE_150.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

/// Wraps an internal gRPC message with the service credential when production
/// service authentication is enabled.
pub fn grpc_service_request<T>(message: T) -> Result<tonic::Request<T>, GrpcServiceAuthError> {
    let mut request = tonic::Request::new(message);
    if !service_auth_required() {
        return Ok(request);
    }
    let token = env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return Err(GrpcServiceAuthError::MissingToken);
    }
    let value = tonic::metadata::MetadataValue::try_from(token.as_str())
        .map_err(|_| GrpcServiceAuthError::InvalidToken)?;
    request.metadata_mut().insert("x-service-token", value);
    Ok(request)
}

/// Tonic interceptor for business-only gRPC services. Health is registered as
/// a separate service and therefore remains available to infrastructure probes.
#[allow(clippy::result_large_err)] // Tonic interceptor requires tonic::Status.
pub fn grpc_service_auth_interceptor(
    request: tonic::Request<()>,
) -> Result<tonic::Request<()>, tonic::Status> {
    if !service_auth_required() {
        return Ok(request);
    }
    let expected = env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
    let actual = request
        .metadata()
        .get("x-service-token")
        .and_then(|value| value.to_str().ok());
    if !valid_service_token(&expected, actual) {
        return Err(tonic::Status::unauthenticated(
            "invalid service credentials",
        ));
    }
    Ok(request)
}

fn service_auth_required() -> bool {
    env::var("SERVICE_AUTH_REQUIRED").is_ok_and(|value| value == "true")
}

fn valid_service_token(expected: &str, actual: Option<&str>) -> bool {
    !expected.is_empty() && actual == Some(expected)
}

async fn service_auth(service: &'static str, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_probe = matches!(path, "/health" | "/ready" | "/metrics");
    if service_auth_required() && service != "gateway" && !is_probe {
        let expected = env::var("SERVICE_AUTH_TOKEN").unwrap_or_default();
        let actual = request
            .headers()
            .get("x-service-token")
            .and_then(|v| v.to_str().ok());
        if !valid_service_token(&expected, actual) {
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
    #[serde(rename = "exp")]
    _exp: usize,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Clone)]
struct JwksSnapshot {
    url: String,
    fetched_at: std::time::Instant,
    keys: jsonwebtoken::jwk::JwkSet,
}

static JWKS_SNAPSHOT: tokio::sync::RwLock<Option<JwksSnapshot>> =
    tokio::sync::RwLock::const_new(None);

fn configured_auth_value(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn jwks_cache_ttl() -> std::time::Duration {
    let seconds = configured_auth_value("AUTH_JWKS_CACHE_SECONDS")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(300);
    std::time::Duration::from_secs(seconds.min(86_400))
}

fn auth_validation() -> jsonwebtoken::Validation {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    // A JWKS issuer should never be accepted with an algorithm chosen by the
    // token. Keep the allow-list explicit and independent of key metadata.
    validation.algorithms = vec![
        jsonwebtoken::Algorithm::RS256,
        jsonwebtoken::Algorithm::RS384,
        jsonwebtoken::Algorithm::RS512,
        jsonwebtoken::Algorithm::ES256,
        jsonwebtoken::Algorithm::ES384,
        jsonwebtoken::Algorithm::EdDSA,
    ];
    if let Some(issuer) = configured_auth_value("AUTH_ISSUER") {
        validation.set_issuer(&[issuer]);
        validation.required_spec_claims.insert("iss".to_string());
    }
    if let Some(audience) = configured_auth_value("AUTH_AUDIENCE") {
        validation.set_audience(&[audience]);
        validation.required_spec_claims.insert("aud".to_string());
    }
    validation
}

async fn fetch_jwks(url: &str) -> Result<jsonwebtoken::jwk::JwkSet, ()> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(500))
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|_| ())?;
    client
        .get(url)
        .send()
        .await
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?
        .json::<jsonwebtoken::jwk::JwkSet>()
        .await
        .map_err(|_| ())
}

async fn jwks_key(kid: &str, force_refresh: bool) -> Option<jsonwebtoken::DecodingKey> {
    let url = configured_auth_value("AUTH_JWKS_URL")?;
    let now = std::time::Instant::now();
    let cached = JWKS_SNAPSHOT.read().await.clone();
    let needs_refresh = force_refresh
        || cached
            .as_ref()
            .is_none_or(|snapshot| {
                snapshot.url != url
                    || now.duration_since(snapshot.fetched_at) >= jwks_cache_ttl()
            });
    if needs_refresh {
        // Serialize refreshes so a key rotation cannot stampede the identity
        // provider when many requests arrive with the new `kid`.
        let mut guard = JWKS_SNAPSHOT.write().await;
        let still_fresh = guard.as_ref().is_some_and(|snapshot| {
            !force_refresh
                && snapshot.url == url
                && now.duration_since(snapshot.fetched_at) < jwks_cache_ttl()
        });
        if !still_fresh {
            let fetched = fetch_jwks(&url).await.ok()?;
            *guard = Some(JwksSnapshot {
                url: url.clone(),
                fetched_at: std::time::Instant::now(),
                keys: fetched,
            });
        }
    }
    let guard = JWKS_SNAPSHOT.read().await;
    guard
        .as_ref()?
        .keys
        .find(kid)
        .and_then(|jwk| jsonwebtoken::DecodingKey::from_jwk(jwk).ok())
}

async fn decode_bearer(token: &str) -> Option<Claims> {
    if configured_auth_value("AUTH_JWKS_URL").is_some() {
        let header = jsonwebtoken::decode_header(token).ok()?;
        let kid = header.kid.as_deref()?;
        let key = match jwks_key(kid, false).await {
            Some(key) => key,
            None => jwks_key(kid, true).await?,
        };
        let mut validation = auth_validation();
        validation
            .algorithms
            .retain(|algorithm| *algorithm == header.alg);
        if validation.algorithms.is_empty() {
            return None;
        }
        jsonwebtoken::decode::<Claims>(token, &key, &validation)
            .ok()
            .map(|data| data.claims)
    } else {
        let secret = configured_auth_value("AUTH_JWT_SECRET")?;
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        if let Some(issuer) = configured_auth_value("AUTH_ISSUER") {
            validation.set_issuer(&[issuer]);
            validation.required_spec_claims.insert("iss".to_string());
        }
        if let Some(audience) = configured_auth_value("AUTH_AUDIENCE") {
            validation.set_audience(&[audience]);
            validation.required_spec_claims.insert("aud".to_string());
        }
        jsonwebtoken::decode::<Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .ok()
        .map(|data| data.claims)
    }
}

async fn auth_user(service: &'static str, mut request: Request, next: Next) -> Response {
    let required = env::var("AUTH_REQUIRED").is_ok_and(|value| value == "true");
    if service != "gateway"
        || !required
        || request.method() == Method::OPTIONS
        || !request.uri().path().starts_with("/v1/")
    {
        return next.run(request).await;
    }
    // Authenticated identity is the only trusted source for these internal headers.
    request.headers_mut().remove("x-user-id");
    request.headers_mut().remove("x-user-roles");
    if configured_auth_value("AUTH_JWKS_URL").is_none()
        && configured_auth_value("AUTH_JWT_SECRET").is_none()
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "AUTH_JWT_SECRET or AUTH_JWKS_URL is required\n",
        )
            .into_response();
    }
    let Some(token) = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return (StatusCode::UNAUTHORIZED, "bearer token required\n").into_response();
    };
    let claims = match decode_bearer(token).await {
        Some(claims) if !claims.sub.trim().is_empty() => claims,
        _ => return (StatusCode::UNAUTHORIZED, "invalid bearer token\n").into_response(),
    };
    let user_id = match HeaderValue::try_from(claims.sub) {
        Ok(value) => value,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid bearer token\n").into_response(),
    };
    request.headers_mut().insert("x-user-id", user_id);
    if let Some(roles) = trusted_roles_header(claims.roles) {
        request.headers_mut().insert("x-user-roles", roles);
    }
    next.run(request).await
}

fn trusted_roles_header(roles: Vec<String>) -> Option<HeaderValue> {
    let roles = roles
        .into_iter()
        .map(|role| role.trim().to_ascii_lowercase())
        .filter(|role| {
            !role.is_empty()
                && role.len() <= 64
                && role
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .collect::<Vec<_>>();
    if !roles.is_empty()
        && let Ok(roles) = HeaderValue::try_from(roles.join(","))
    {
        return Some(roles);
    }
    None
}

async fn rate_limit(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if request.method() == Method::OPTIONS
        || matches!(path.as_str(), "/health" | "/ready" | "/metrics")
        || env::var("REDIS_URL").is_err()
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
    let script = redis::Script::new(RATE_LIMIT_LUA);
    let mut invocation = script.prepare_invoke();
    invocation.key(&key).arg(60);
    let count = tokio::time::timeout(
        command_timeout,
        invocation.invoke_async::<u64>(&mut connection),
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

#[cfg(test)]
mod tests {
    use super::{trusted_roles_header, valid_service_token};

    #[test]
    fn injects_only_normalized_safe_roles() {
        let header = trusted_roles_header(vec![
            " Moderator ".to_string(),
            "trust_safety".to_string(),
            "admin,spoofed".to_string(),
            "moderator role".to_string(),
        ])
        .expect("safe roles");

        assert_eq!(header, "moderator,trust_safety");
    }

    #[test]
    fn rejects_missing_or_mismatched_service_tokens() {
        assert!(valid_service_token("service-token", Some("service-token")));
        assert!(!valid_service_token("", Some("service-token")));
        assert!(!valid_service_token("service-token", None));
        assert!(!valid_service_token("service-token", Some("wrong-token")));
    }
}
