use std::{
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use tonic::transport::{Channel, Endpoint};
use tracing::warn;

/// Outbound gRPC transport failure with the upstream URL preserved for logs.
#[derive(Debug, thiserror::Error)]
pub enum ConnectFailure {
    #[error("grpc upstream connect failed for {url}: {source}")]
    Connect {
        url: String,
        source: tonic::transport::Error,
    },
}

/// Connects an outbound gRPC channel with production-grade transport hygiene:
/// bounded connect time, HTTP/2 keep-alive probes and TCP_NODELAY.
///
/// Domain code keeps calling the tonic-generated clients directly
/// (`PbClient::new(grpc_channel(&url).await?)`); this only hardens the shared
/// transport instead of introducing a forwarding client wrapper.
pub async fn grpc_channel(url: &str) -> Result<Channel, ConnectFailure> {
    let owned = url.to_string();
    let endpoint = Endpoint::from_shared(owned.clone());
    let endpoint = match endpoint {
        Ok(endpoint) => endpoint,
        Err(source) => return Err(ConnectFailure::Connect { url: owned, source }),
    };
    endpoint
        .connect_timeout(env_duration("GRPC_CONNECT_TIMEOUT_MS", 500))
        .tcp_nodelay(true)
        .http2_keep_alive_interval(env_duration("GRPC_KEEP_ALIVE_INTERVAL_MS", 30_000))
        .keep_alive_timeout(env_duration("GRPC_KEEP_ALIVE_TIMEOUT_MS", 10_000))
        .keep_alive_while_idle(true)
        .connect()
        .await
        .map_err(|source| {
            warn!(%url, %source, "grpc upstream connect failed");
            ConnectFailure::Connect { url: owned, source }
        })
}

/// Per-upstream consecutive-failure circuit breaker for read-mostly unary calls.
///
/// While OPEN it fails fast with `Unavailable` so a dead dependency cannot eat
/// the caller's latency budget; after the cooldown one probe call is allowed
/// through (half-open) before traffic fully resumes on success.
#[derive(Debug)]
pub struct CircuitBreaker {
    threshold: usize,
    cooldown: Duration,
    state: Mutex<BreakerState>,
    probing: AtomicBool,
}

#[derive(Debug, Default)]
struct BreakerState {
    consecutive_failures: usize,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn from_env() -> Self {
        let threshold = std::env::var("GRPC_BREAKER_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| *value > 0)
            .unwrap_or(5);
        let cooldown = env_duration("GRPC_BREAKER_COOLDOWN_MS", 5_000);
        Self::new(threshold, cooldown)
    }

    pub fn new(threshold: usize, cooldown: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown,
            state: Mutex::new(BreakerState::default()),
            probing: AtomicBool::new(false),
        }
    }

    /// Runs one dependent call under breaker accounting.
    /// Err(Unavailable) means the breaker rejected the call without dialing.
    pub async fn execute<T, F>(&self, call: F) -> Result<T, tonic::Status>
    where
        F: Future<Output = Result<T, tonic::Status>>,
    {
        if !self.admit() {
            return Err(tonic::Status::unavailable(
                "upstream circuit breaker is open",
            ));
        }
        let result = call.await;
        match &result {
            Ok(_) => self.record_success(),
            Err(status) if is_breakable(status) => self.record_failure(),
            Err(_) => self.record_success(),
        }
        result
    }

    fn admit(&self) -> bool {
        let Ok(state) = self.state.lock() else {
            return true; // a poisoned lock must not wedge all traffic
        };
        match state.opened_at {
            None => true,
            Some(opened_at) if opened_at.elapsed() < self.cooldown => false,
            // Past the cooldown: half-open, admitting exactly one probe call.
            Some(_) => {
                drop(state);
                self.probing
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            }
        }
    }

    fn record_success(&self) {
        self.probing.store(false, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            state.consecutive_failures = 0;
            state.opened_at = None;
        }
    }

    fn record_failure(&self) {
        self.probing.store(false, Ordering::Release);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.consecutive_failures += 1;
        if state.consecutive_failures >= self.threshold && state.opened_at.is_none() {
            state.opened_at = Some(Instant::now());
        }
    }

    #[cfg(test)]
    fn failures(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.consecutive_failures)
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn is_open(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.opened_at.is_some())
            .unwrap_or(false)
    }
}

fn is_breakable(status: &tonic::Status) -> bool {
    matches!(
        status.code(),
        tonic::Code::Unavailable | tonic::Code::DeadlineExceeded | tonic::Code::Internal
    )
}

fn env_duration(key: &str, default_ms: u64) -> Duration {
    Duration::from_millis(
        std::env::var(key)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default_ms),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ok_call() -> Result<(), tonic::Status> {
        Ok(())
    }

    async fn fail_call() -> Result<(), tonic::Status> {
        Err(tonic::Status::unavailable("down"))
    }

    #[tokio::test]
    async fn stays_closed_below_threshold() {
        let breaker = CircuitBreaker::new(3, Duration::from_millis(50));
        for _ in 0..2 {
            let _ = breaker.execute(fail_call()).await;
        }
        assert!(!breaker.is_open());
        assert!(breaker.execute(ok_call()).await.is_ok());
        assert_eq!(breaker.failures(), 0);
    }

    #[tokio::test]
    async fn zero_threshold_is_clamped_to_one() {
        let breaker = CircuitBreaker::new(0, Duration::from_millis(50));
        let _ = breaker.execute(fail_call()).await;
        assert!(breaker.is_open());
    }

    #[tokio::test]
    async fn opens_at_threshold_and_fails_fast() {
        let breaker = CircuitBreaker::new(3, Duration::from_millis(50));
        for _ in 0..3 {
            let _ = breaker.execute(fail_call()).await;
        }
        assert!(breaker.is_open());
        let error = breaker
            .execute(ok_call())
            .await
            .expect_err("open breaker must reject calls");
        assert_eq!(error.code(), tonic::Code::Unavailable);
    }

    #[tokio::test]
    async fn recovers_after_cooldown_with_probe() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(20));
        for _ in 0..2 {
            let _ = breaker.execute(fail_call()).await;
        }
        assert!(breaker.is_open());
        tokio::time::sleep(Duration::from_millis(40)).await;
        // First post-cooldown call is admitted as the probe.
        assert!(breaker.execute(ok_call()).await.is_ok());
        assert!(!breaker.is_open());
        assert_eq!(breaker.failures(), 0);
    }

    #[tokio::test]
    async fn ignores_non_transport_errors() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(50));
        let denied: Result<(), tonic::Status> = Err(tonic::Status::permission_denied("no"));
        for _ in 0..5 {
            let _ = breaker.execute(async { denied.clone() }).await;
        }
        assert!(!breaker.is_open());
    }
}
