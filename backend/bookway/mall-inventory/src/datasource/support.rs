use crate::api::pb;
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use std::collections::{BTreeMap, HashMap};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;

const EXPIRY_SWEEP_LIMIT: usize = 1_000;
const DEFAULT_REDIS_CACHE_TTL_SECONDS: u64 = 300;
const RESERVATION_CACHE_TTL_SECONDS: u64 = 86_400;

// Redis only accelerates the reserve gate. PostgreSQL remains the durable
// source of truth, so cache eviction or a Redis failover cannot sell stock.
const RESERVE_LUA: &str = r#"
if redis.call('EXISTS', KEYS[1]) == 1 then return 2 end
local cache_ttl = tonumber(ARGV[1])
for index = 2, #KEYS do
  local value = redis.call('GET', KEYS[index])
  if not value then return -1 end
  local delimiter = string.find(value, ':')
  if not delimiter then return -1 end
  local available = tonumber(string.sub(value, 1, delimiter - 1))
  local reserved = tonumber(string.sub(value, delimiter + 1))
  local quantity = tonumber(ARGV[index])
  if not available or not reserved or not quantity then return -1 end
  if available - reserved < quantity then return -2 end
end
for index = 2, #KEYS do
  local value = redis.call('GET', KEYS[index])
  local delimiter = string.find(value, ':')
  local available = tonumber(string.sub(value, 1, delimiter - 1))
  local reserved = tonumber(string.sub(value, delimiter + 1))
  local quantity = tonumber(ARGV[index])
  redis.call('SET', KEYS[index], available .. ':' .. (reserved + quantity), 'EX', cache_ttl)
end
redis.call('SET', KEYS[1], 'reserved', 'EX', ARGV[#ARGV])
return 1
"#;

const ROLLBACK_LUA: &str = r#"
if redis.call('EXISTS', KEYS[1]) == 0 then return 0 end
for index = 2, #KEYS do
  local value = redis.call('GET', KEYS[index])
  if not value then return -1 end
  local delimiter = string.find(value, ':')
  if not delimiter then return -1 end
  local available = tonumber(string.sub(value, 1, delimiter - 1))
  local reserved = tonumber(string.sub(value, delimiter + 1))
  local quantity = tonumber(ARGV[index - 1])
  if not available or not reserved or not quantity or reserved < quantity then return -1 end
end
for index = 2, #KEYS do
  local value = redis.call('GET', KEYS[index])
  local delimiter = string.find(value, ':')
  local available = tonumber(string.sub(value, 1, delimiter - 1))
  local reserved = tonumber(string.sub(value, delimiter + 1))
  local quantity = tonumber(ARGV[index - 1])
  redis.call('SET', KEYS[index], available .. ':' .. (reserved - quantity), 'EX', ARGV[#ARGV])
end
redis.call('DEL', KEYS[1])
return 1
"#;

#[derive(Debug)]
pub(crate) enum DaoError {
    NotFound(String),
    Insufficient(String),
    Conflict(String),
    Failed(String),
}
#[async_trait]
pub(crate) trait InventoryDao: Send + Sync {
    async fn set_stock(&self, request: pb::SetStockRequest) -> Result<pb::InventoryItem, DaoError>;
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, DaoError>;
    async fn reserve(&self, request: pb::ReserveRequest) -> Result<pb::Reservation, DaoError>;
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, DaoError>;
    async fn release(&self, id: &str) -> Result<pb::Reservation, DaoError>;
    async fn expire(&self, limit: usize) -> Result<u32, DaoError>;
}
#[derive(Default)]
struct MemoryState {
    stock: HashMap<String, pb::InventoryItem>,
    reservations: HashMap<String, MemoryReservation>,
}
#[derive(Clone)]
struct MemoryReservation {
    status: String,
    expires_at: OffsetDateTime,
    items: Vec<pb::ReservationLine>,
}

fn reservation_proto(id: &str, value: &MemoryReservation) -> Result<pb::Reservation, DaoError> {
    Ok(pb::Reservation {
        id: id.to_string(),
        status: value.status.clone(),
        expires_at: timestamp(value.expires_at),
        items: value.items.clone(),
    })
}
fn quantities(items: &[pb::ReservationLine]) -> Result<BTreeMap<String, i64>, DaoError> {
    let mut values = BTreeMap::new();
    for item in items {
        let entry = values.entry(item.sku_id.clone()).or_insert(0_i64);
        *entry = entry
            .checked_add(i64::from(item.quantity))
            .ok_or_else(|| DaoError::Failed("inventory quantity overflow".to_string()))?;
    }
    Ok(values)
}

fn same_reservation_items(
    existing: &[pb::ReservationLine],
    requested: &[pb::ReservationLine],
) -> Result<bool, DaoError> {
    Ok(quantities(existing)? == quantities(requested)?)
}

fn reservation_conflict() -> DaoError {
    DaoError::Conflict("reservation id is already bound to different inventory items".to_string())
}

fn stock_cache_key(sku_id: &str) -> String {
    format!("bookway:inventory:stock:{sku_id}")
}

fn reservation_cache_key(reservation_id: &str) -> String {
    format!("bookway:inventory:reservation:{reservation_id}")
}

fn stock_cache_value(stock: &pb::InventoryItem) -> String {
    format!("{}:{}", stock.available, stock.reserved)
}

fn valid_stock_cache_value(value: &str) -> bool {
    let Some((available, reserved)) = value.split_once(':') else {
        return false;
    };
    let (Ok(available), Ok(reserved)) = (available.parse::<i64>(), reserved.parse::<i64>()) else {
        return false;
    };
    available >= 0 && reserved >= 0 && reserved <= available
}

fn expire_memory(state: &mut MemoryState, limit: usize) -> u32 {
    let expired = state
        .reservations
        .iter()
        .filter(|(_, reservation)| {
            reservation.status == "reserved" && reservation.expires_at <= OffsetDateTime::now_utc()
        })
        .map(|(id, _)| id.clone())
        .take(limit)
        .collect::<Vec<_>>();
    let count = u32::try_from(expired.len()).unwrap_or(u32::MAX);
    for id in expired {
        if let Some(reservation) = state.reservations.get_mut(&id) {
            for item in &reservation.items {
                if let Some(stock) = state.stock.get_mut(&item.sku_id) {
                    stock.reserved = stock.reserved.saturating_sub(i64::from(item.quantity));
                    stock.updated_at = timestamp(OffsetDateTime::now_utc());
                }
            }
            reservation.status = "expired".to_string();
        }
    }
    count
}
async fn expire_postgres(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    limit: usize,
) -> Result<u32, DaoError> {
    // Claim a bounded batch so parallel sweepers release each reservation once.
    let expired = sqlx::query_scalar::<_, i64>("WITH candidates AS MATERIALIZED (SELECT id FROM mall_inventory_reservations WHERE status='reserved' AND expires_at <= now() ORDER BY expires_at,id FOR UPDATE SKIP LOCKED LIMIT $1), expired AS (UPDATE mall_inventory_reservations r SET status='expired',updated_at=now() FROM candidates WHERE r.id=candidates.id RETURNING r.id), released AS (SELECT i.sku_id, sum(i.quantity) AS quantity FROM mall_inventory_reservation_items i INNER JOIN expired e ON e.id=i.reservation_id GROUP BY i.sku_id), adjusted AS (UPDATE mall_inventory_stock s SET reserved=GREATEST(0,s.reserved-released.quantity),updated_at=now() FROM released WHERE s.sku_id=released.sku_id RETURNING s.sku_id) SELECT count(*) FROM expired")
        .bind(i64::try_from(limit.clamp(1, EXPIRY_SWEEP_LIMIT)).unwrap_or(1_000))
        .fetch_one(&mut **tx)
        .await
        .map_err(database)?;
    Ok(u32::try_from(expired).unwrap_or(u32::MAX))
}
fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
fn database(error: sqlx::Error) -> DaoError {
    DaoError::Failed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{DaoError, InventoryDao, MemoryInventoryDao, valid_stock_cache_value};
    use crate::api::pb;

    #[tokio::test]
    async fn reservation_is_idempotent_and_confirmation_consumes_stock() {
        let Dao = MemoryInventoryDao::default();
        Dao.set_stock(pb::SetStockRequest {
            sku_id: "sku-1".to_string(),
            available: 2,
        })
        .await
        .expect("stock should be initialized");
        let request = pb::ReserveRequest {
            reservation_id: "order-1".to_string(),
            items: vec![pb::ReservationLine {
                sku_id: "sku-1".to_string(),
                quantity: 2,
            }],
            ttl_seconds: Some(900),
        };
        let first = Dao
            .reserve(request.clone())
            .await
            .expect("stock should reserve");
        let retry = Dao
            .reserve(request)
            .await
            .expect("same order should return its reservation");
        assert_eq!(first.id, retry.id);
        let conflict = Dao
            .reserve(pb::ReserveRequest {
                reservation_id: "order-1".to_string(),
                items: vec![pb::ReservationLine {
                    sku_id: "sku-1".to_string(),
                    quantity: 1,
                }],
                ttl_seconds: Some(900),
            })
            .await
            .expect_err("a reservation id cannot be reused for a different payload");
        assert!(matches!(conflict, DaoError::Conflict(_)));
        assert_eq!(Dao.stock("sku-1").await.expect("stock exists").reserved, 2);

        let error = Dao
            .reserve(pb::ReserveRequest {
                reservation_id: "order-2".to_string(),
                items: vec![pb::ReservationLine {
                    sku_id: "sku-1".to_string(),
                    quantity: 1,
                }],
                ttl_seconds: Some(900),
            })
            .await
            .expect_err("reserved stock must not be oversold");
        assert!(matches!(error, DaoError::Insufficient(_)));

        Dao.confirm("order-1")
            .await
            .expect("reservation should commit");
        let stock = Dao.stock("sku-1").await.expect("stock exists");
        assert_eq!(stock.available, 0);
        assert_eq!(stock.reserved, 0);

        let released = Dao
            .release("order-without-reservation")
            .await
            .expect("compensation for a failed initial reservation is idempotent");
        assert_eq!(released.status, "released");
    }

    #[tokio::test]
    async fn sweep_releases_expired_reservations_without_a_follow_up_checkout() {
        let Dao = MemoryInventoryDao::default();
        Dao.set_stock(pb::SetStockRequest {
            sku_id: "sku-1".to_string(),
            available: 1,
        })
        .await
        .expect("stock should be initialized");
        Dao.reserve(pb::ReserveRequest {
            reservation_id: "order-1".to_string(),
            items: vec![pb::ReservationLine {
                sku_id: "sku-1".to_string(),
                quantity: 1,
            }],
            ttl_seconds: Some(0),
        })
        .await
        .expect("stock should reserve");

        assert_eq!(Dao.expire(100).await.expect("sweep should work"), 1);
        assert_eq!(
            Dao.stock("sku-1")
                .await
                .expect("stock should exist")
                .reserved,
            0
        );
    }

    #[test]
    fn stock_cache_payload_requires_a_non_negative_consistent_snapshot() {
        assert!(valid_stock_cache_value("12:3"));
        assert!(!valid_stock_cache_value("12:13"));
        assert!(!valid_stock_cache_value("12:-1"));
        assert!(!valid_stock_cache_value("not-a-stock-value"));
    }
}

#[path = "memory_inventory_dao.rs"]
mod memory_inventory_dao;
pub(crate) use memory_inventory_dao::MemoryInventoryDao;
#[path = "postgres_inventory_dao.rs"]
mod postgres_inventory_dao;
pub(crate) use postgres_inventory_dao::PostgresInventoryDao;
#[path = "redis_inventory_dao.rs"]
mod redis_inventory_dao;
pub(crate) use redis_inventory_dao::RedisInventoryDao;
