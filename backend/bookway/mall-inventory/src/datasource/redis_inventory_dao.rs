use super::*;

use std::{
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::time::sleep;

#[derive(Clone)]
pub(crate) struct RedisInventoryDao {
    postgres: PostgresInventoryDao,
    redis: ConnectionManager,
    cache_ttl_seconds: u64,
    // Coalesce concurrent cache misses for the same SKU before they reach
    // PostgreSQL. Weak references keep this map bounded by active readers.
    stock_miss_locks: Arc<Mutex<std::collections::HashMap<String, Weak<Mutex<()>>>>>,
}

#[async_trait]
impl InventoryDao for RedisInventoryDao {
    async fn set_stock(&self, request: pb::SetStockRequest) -> Result<pb::InventoryItem, DaoError> {
        let stock = self.postgres.set_stock(request).await?;
        if let Err(error) = self.cache_stock(&stock).await {
            tracing::warn!(%error, sku_id = %stock.sku_id, "inventory Redis cache write degraded");
        }
        Ok(stock)
    }

    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, DaoError> {
        if let Some(stock) = self.load_cached_stock(sku_id).await {
            return Ok(stock);
        }

        let local = self.stock_miss_lock(sku_id).await;
        if let Some(stock) = self.load_cached_stock(sku_id).await {
            return Ok(stock);
        }

        // A short distributed lease prevents every inventory instance from
        // refreshing the same cold SKU simultaneously. A peer that owns the
        // lease gets a bounded chance to publish the value; if it does not,
        // this request still falls back to the durable source of truth.
        let lease = self.acquire_stock_refresh_lease(sku_id).await;
        if lease.is_peer() {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(80);
            while tokio::time::Instant::now() < deadline {
                sleep(Duration::from_millis(10)).await;
                if let Some(stock) = self.load_cached_stock(sku_id).await {
                    return Ok(stock);
                }
            }
        }

        let result = self.postgres.stock(sku_id).await;
        if let Ok(stock) = &result
            && let Err(error) = self.cache_stock(stock).await
        {
            tracing::debug!(%error, sku_id, "inventory stock cache write degraded");
        }
        lease.release().await;
        drop(local);
        result
    }

    async fn reserve(&self, request: pb::ReserveRequest) -> Result<pb::Reservation, DaoError> {
        match self
            .existing_cached_reservation(&request.reservation_id)
            .await
        {
            Ok(true) => match self.postgres.reservation(&request.reservation_id).await {
                Ok(reservation) => {
                    if !same_reservation_items(&reservation.items, &request.items)? {
                        return Err(reservation_conflict());
                    }
                    return Ok(reservation);
                }
                Err(DaoError::NotFound(_)) => {
                    if let Err(error) = self.clear_reservation_cache(&request.reservation_id).await
                    {
                        tracing::warn!(%error, reservation_id = %request.reservation_id, "stale inventory reservation cache cleanup degraded");
                    }
                    self.invalidate_stock_cache(&request.items).await;
                }
                Err(error) => return Err(error),
            },
            Ok(false) => match self.postgres.reservation(&request.reservation_id).await {
                // Redis may have been evicted or restarted after the durable
                // write. Never apply a second cache hold to that reservation.
                Ok(reservation) => {
                    if !same_reservation_items(&reservation.items, &request.items)? {
                        return Err(reservation_conflict());
                    }
                    return Ok(reservation);
                }
                Err(DaoError::NotFound(_)) => {}
                Err(error) => return Err(error),
            },
            Err(error) => {
                tracing::warn!(%error, reservation_id = %request.reservation_id, "inventory Redis reservation lookup degraded; using PostgreSQL");
                return self.postgres.reserve(request).await;
            }
        }

        match self.reserve_cache_gate(&request).await {
            Ok(1) => match self.postgres.reserve(request.clone()).await {
                Ok(reservation) => {
                    // The Redis gate and durable insert are deliberately
                    // separate. Another writer may have committed the same
                    // reservation between the initial lookup and the gate;
                    // reconcile from PostgreSQL so the cache cannot retain a
                    // double-held reserved count in that race.
                    self.reconcile_cached_stock(&reservation.items).await;
                    Ok(reservation)
                }
                Err(error) => {
                    if let Err(rollback_error) = self.rollback_cache_gate(&request).await {
                        tracing::error!(%rollback_error, reservation_id = %request.reservation_id, "inventory Redis reservation compensation failed");
                        self.invalidate_stock_cache(&request.items).await;
                    }
                    Err(error)
                }
            },
            Ok(2) => match self.postgres.reservation(&request.reservation_id).await {
                Ok(reservation) => {
                    if !same_reservation_items(&reservation.items, &request.items)? {
                        Err(reservation_conflict())
                    } else {
                        Ok(reservation)
                    }
                }
                Err(DaoError::NotFound(_)) => {
                    self.invalidate_stock_cache(&request.items).await;
                    self.postgres.reserve(request).await
                }
                Err(error) => Err(error),
            },
            Ok(-2) => {
                // A stale cache may report insufficient stock after an expiry,
                // release, or restock. PostgreSQL remains authoritative, so
                // verify the decision there instead of turning cache lag into
                // a false rejection.
                match self.postgres.reserve(request.clone()).await {
                    Ok(reservation) => {
                        self.reconcile_cached_stock(&reservation.items).await;
                        Ok(reservation)
                    }
                    Err(error) => {
                        self.invalidate_stock_cache(&request.items).await;
                        Err(error)
                    }
                }
            }
            Ok(result) => {
                tracing::warn!(result, reservation_id = %request.reservation_id, "inventory Redis cache was incomplete; using PostgreSQL");
                self.postgres.reserve(request).await
            }
            Err(error) => {
                tracing::warn!(%error, reservation_id = %request.reservation_id, "inventory Redis reservation gate degraded; using PostgreSQL");
                self.postgres.reserve(request).await
            }
        }
    }

    async fn confirm(&self, id: &str) -> Result<pb::Reservation, DaoError> {
        let reservation = self.postgres.confirm(id).await?;
        self.reconcile_cached_stock(&reservation.items).await;
        Ok(reservation)
    }

    async fn release(&self, id: &str) -> Result<pb::Reservation, DaoError> {
        let reservation = self.postgres.release(id).await?;
        self.reconcile_cached_stock(&reservation.items).await;
        Ok(reservation)
    }

    async fn expire(&self, limit: usize) -> Result<u32, DaoError> {
        let expired = self.postgres.expire(limit).await?;
        // Expiry may touch arbitrary SKU lines. Let subsequent reservations
        // rehydrate instead of serving an expired cached reserved count.
        if expired > 0
            && let Err(error) = self.clear_expiry_cache().await
        {
            tracing::warn!(%error, "inventory Redis expiry cache cleanup degraded");
        }
        Ok(expired)
    }
}

impl RedisInventoryDao {
    pub(crate) fn new(postgres: PostgresInventoryDao, redis: ConnectionManager) -> Self {
        Self {
            postgres,
            redis,
            cache_ttl_seconds: std::env::var("MALL_INVENTORY_REDIS_CACHE_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_REDIS_CACHE_TTL_SECONDS)
                .clamp(30, 3_600),
            stock_miss_locks: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    async fn stock_miss_lock(&self, sku_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.stock_miss_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            locks
                .get(sku_id)
                .and_then(Weak::upgrade)
                .unwrap_or_else(|| {
                    let lock = Arc::new(Mutex::new(()));
                    locks.insert(sku_id.to_string(), Arc::downgrade(&lock));
                    lock
                })
        };
        lock.lock_owned().await
    }

    async fn load_cached_stock(&self, sku_id: &str) -> Option<pb::InventoryItem> {
        let mut redis = self.redis.clone();
        let value: redis::RedisResult<Option<String>> = redis::cmd("GET")
            .arg(stock_cache_key(sku_id))
            .query_async(&mut redis)
            .await;
        let value = match value {
            Ok(Some(value)) => value,
            Ok(None) => return None,
            Err(error) => {
                tracing::debug!(%error, sku_id, "inventory stock cache read degraded");
                return None;
            }
        };
        let stock = stock_from_cache_value(sku_id, &value);
        if stock.is_none() {
            tracing::warn!(sku_id, "inventory stock cache payload invalid");
        }
        stock
    }

    async fn acquire_stock_refresh_lease(&self, sku_id: &str) -> StockRefreshLease {
        let key = format!("bookway:inventory:stock-refresh:{sku_id}");
        let token = format!("{}-{}", std::process::id(), uuid::Uuid::now_v7());
        let mut redis = self.redis.clone();
        match redis::cmd("SET")
            .arg(&key)
            .arg(&token)
            .arg("NX")
            .arg("PX")
            .arg(STOCK_REFRESH_LOCK_TTL_MS)
            .query_async::<Option<String>>(&mut redis)
            .await
        {
            Ok(Some(_)) => StockRefreshLease::Owned { redis, key, token },
            Ok(None) => StockRefreshLease::Peer,
            Err(error) => {
                tracing::debug!(%error, sku_id, "inventory stock refresh lease degraded");
                StockRefreshLease::Uncoordinated
            }
        }
    }

    async fn cache_stock(&self, stock: &pb::InventoryItem) -> redis::RedisResult<()> {
        let mut redis = self.redis.clone();
        redis::cmd("SET")
            .arg(stock_cache_key(&stock.sku_id))
            .arg(stock_cache_value(stock))
            .arg("EX")
            .arg(self.cache_ttl_seconds)
            .query_async(&mut redis)
            .await
    }

    async fn seed_stock_cache(&self, items: &[pb::ReservationLine]) -> redis::RedisResult<()> {
        let keys = items
            .iter()
            .map(|item| stock_cache_key(&item.sku_id))
            .collect::<Vec<_>>();
        let mut redis = self.redis.clone();
        let cached: Vec<Option<String>> = redis::cmd("MGET")
            .arg(&keys)
            .query_async(&mut redis)
            .await?;
        for (item, cached) in items.iter().zip(cached) {
            if cached.as_deref().is_some_and(valid_stock_cache_value) {
                continue;
            }
            let stock = self.postgres.stock(&item.sku_id).await.map_err(|error| {
                redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "inventory stock",
                    format!("{error:?}"),
                ))
            })?;
            self.cache_stock(&stock).await?;
        }
        Ok(())
    }

    async fn clear_reservation_cache(&self, id: &str) -> redis::RedisResult<()> {
        let mut redis = self.redis.clone();
        redis::cmd("DEL")
            .arg(reservation_cache_key(id))
            .query_async(&mut redis)
            .await
    }

    async fn invalidate_stock_cache(&self, items: &[pb::ReservationLine]) {
        let keys = items
            .iter()
            .map(|item| stock_cache_key(&item.sku_id))
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return;
        }
        let mut redis = self.redis.clone();
        let result: redis::RedisResult<()> =
            redis::cmd("DEL").arg(keys).query_async(&mut redis).await;
        if let Err(error) = result {
            tracing::warn!(%error, "inventory Redis cache invalidation degraded");
        }
    }

    async fn reconcile_cached_stock(&self, items: &[pb::ReservationLine]) {
        for item in items {
            match self.postgres.stock(&item.sku_id).await {
                Ok(stock) => {
                    if let Err(error) = self.cache_stock(&stock).await {
                        tracing::warn!(%error, sku_id = %item.sku_id, "inventory Redis cache reconciliation degraded");
                    }
                }
                Err(error) => {
                    tracing::warn!(error = ?error, sku_id = %item.sku_id, "inventory stock reconciliation degraded")
                }
            }
        }
    }

    async fn reserve_cache_gate(
        &self,
        request: &pb::ReserveRequest,
    ) -> Result<i64, redis::RedisError> {
        self.seed_stock_cache(&request.items).await?;
        let script = redis::Script::new(RESERVE_LUA);
        let mut invocation = script.prepare_invoke();
        invocation.key(reservation_cache_key(&request.reservation_id));
        for item in &request.items {
            invocation.key(stock_cache_key(&item.sku_id));
        }
        invocation.arg(self.cache_ttl_seconds);
        for item in &request.items {
            invocation.arg(item.quantity);
        }
        invocation.arg(RESERVATION_CACHE_TTL_SECONDS);
        let mut redis = self.redis.clone();
        invocation.invoke_async(&mut redis).await
    }

    async fn rollback_cache_gate(&self, request: &pb::ReserveRequest) -> redis::RedisResult<()> {
        let script = redis::Script::new(ROLLBACK_LUA);
        let mut invocation = script.prepare_invoke();
        invocation.key(reservation_cache_key(&request.reservation_id));
        for item in &request.items {
            invocation.key(stock_cache_key(&item.sku_id));
        }
        for item in &request.items {
            invocation.arg(item.quantity);
        }
        invocation.arg(self.cache_ttl_seconds);
        let mut redis = self.redis.clone();
        let _: i64 = invocation.invoke_async(&mut redis).await?;
        Ok(())
    }

    async fn existing_cached_reservation(&self, id: &str) -> redis::RedisResult<bool> {
        let mut redis = self.redis.clone();
        redis::cmd("EXISTS")
            .arg(reservation_cache_key(id))
            .query_async(&mut redis)
            .await
    }

    async fn clear_expiry_cache(&self) -> redis::RedisResult<()> {
        let mut redis = self.redis.clone();
        // SCAN is incremental and may return only one batch even when the
        // database sweep touched more SKUs than the configured count. Drain
        // the full cursor so an expiry pass cannot leave stale reservations
        // cached indefinitely behind the first batch.
        let mut cursor = 0_u64;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg("bookway:inventory:stock:*")
                .arg("COUNT")
                .arg(EXPIRY_SWEEP_LIMIT)
                .query_async(&mut redis)
                .await?;
            if !keys.is_empty() {
                let _: () = redis::cmd("DEL").arg(keys).query_async(&mut redis).await?;
            }
            if next == 0 {
                break;
            }
            cursor = next;
        }
        Ok(())
    }
}

fn stock_from_cache_value(sku_id: &str, value: &str) -> Option<pb::InventoryItem> {
    let (available, reserved) = value.split_once(':')?;
    let available = available.parse::<i64>().ok()?;
    let reserved = reserved.parse::<i64>().ok()?;
    (available >= 0 && reserved >= 0 && reserved <= available).then(|| pb::InventoryItem {
        sku_id: sku_id.to_string(),
        available,
        reserved,
        // The compact Lua payload intentionally omits a mutable timestamp;
        // callers receive the read time rather than an invented durable
        // update time.
        updated_at: timestamp(time::OffsetDateTime::now_utc()),
    })
}

const STOCK_REFRESH_LOCK_TTL_MS: usize = 2_000;
const RELEASE_STOCK_REFRESH_LOCK: &str = r#"
if redis.call('get', KEYS[1]) == ARGV[1] then
  return redis.call('del', KEYS[1])
end
return 0
"#;

enum StockRefreshLease {
    Owned {
        redis: ConnectionManager,
        key: String,
        token: String,
    },
    Peer,
    Uncoordinated,
}

impl StockRefreshLease {
    fn is_peer(&self) -> bool {
        matches!(self, Self::Peer)
    }

    async fn release(self) {
        let Self::Owned {
            mut redis,
            key,
            token,
        } = self
        else {
            return;
        };
        let result: redis::RedisResult<i32> = redis::Script::new(RELEASE_STOCK_REFRESH_LOCK)
            .key(key)
            .arg(token)
            .invoke_async(&mut redis)
            .await;
        if let Err(error) = result {
            tracing::debug!(%error, "inventory stock refresh lease release degraded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stock_from_cache_value;

    #[test]
    fn stock_cache_reads_only_consistent_snapshots() {
        let stock = stock_from_cache_value("sku-1", "12:3").expect("valid cache snapshot");
        assert_eq!(stock.sku_id, "sku-1");
        assert_eq!(stock.available, 12);
        assert_eq!(stock.reserved, 3);
        assert!(!stock.updated_at.is_empty());
        assert!(stock_from_cache_value("sku-1", "12:13").is_none());
        assert!(stock_from_cache_value("sku-1", "-1:0").is_none());
        assert!(stock_from_cache_value("sku-1", "not-a-stock-value").is_none());
    }
}
