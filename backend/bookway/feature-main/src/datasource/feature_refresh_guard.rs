use super::*;

pub(crate) struct FeatureRefreshGuard {
    pub(crate) _local: tokio::sync::OwnedMutexGuard<()>,
    pub(crate) redis: Option<RedisRefreshLease>,
    pub(crate) peer_holds_lease: bool,
}

impl FeatureRefreshGuard {
    pub(crate) fn peer_holds_lease(&self) -> bool {
        self.peer_holds_lease
    }

    pub(crate) async fn release(mut self) {
        if let Some(lease) = self.redis.take() {
            lease.release().await;
        }
    }
}

impl Drop for FeatureRefreshGuard {
    fn drop(&mut self) {
        let Some(lease) = self.redis.take() else {
            return;
        };
        // The normal path explicitly releases the lease. Drop is a bounded
        // best-effort cleanup; the TTL remains the crash-safety backstop.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(lease.release());
        }
    }
}
