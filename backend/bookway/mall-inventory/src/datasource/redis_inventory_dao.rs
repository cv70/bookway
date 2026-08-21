use super::*;

#[derive(Clone)]
pub(crate) struct RedisInventoryDao {
    postgres: PostgresInventoryDao,
    redis: ConnectionManager,
    cache_ttl_seconds: u64,
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
        self.postgres.stock(sku_id).await
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
        let (_, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(0)
            .arg("MATCH")
            .arg("bookway:inventory:stock:*")
            .arg("COUNT")
            .arg(EXPIRY_SWEEP_LIMIT)
            .query_async(&mut redis)
            .await?;
        if keys.is_empty() {
            return Ok(());
        }
        redis::cmd("DEL").arg(keys).query_async(&mut redis).await
    }
}
