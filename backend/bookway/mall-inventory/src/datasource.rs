use crate::api::pb;
use async_trait::async_trait;
use sqlx::{FromRow, PgPool};
use std::collections::{BTreeMap, HashMap};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;

const EXPIRY_SWEEP_LIMIT: usize = 1_000;

#[derive(Debug)]
pub(crate) enum RepositoryError {
    NotFound(String),
    Insufficient(String),
    Failed(String),
}
#[async_trait]
pub(crate) trait InventoryRepository: Send + Sync {
    async fn set_stock(
        &self,
        request: pb::SetStockRequest,
    ) -> Result<pb::InventoryItem, RepositoryError>;
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, RepositoryError>;
    async fn reserve(
        &self,
        request: pb::ReserveRequest,
    ) -> Result<pb::Reservation, RepositoryError>;
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, RepositoryError>;
    async fn release(&self, id: &str) -> Result<pb::Reservation, RepositoryError>;
    async fn expire(&self, limit: usize) -> Result<u32, RepositoryError>;
}
#[derive(Default)]
pub(crate) struct MemoryInventoryRepository {
    state: Mutex<MemoryState>,
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
#[async_trait]
impl InventoryRepository for MemoryInventoryRepository {
    async fn set_stock(
        &self,
        request: pb::SetStockRequest,
    ) -> Result<pb::InventoryItem, RepositoryError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        let current = state
            .stock
            .entry(request.sku_id.clone())
            .or_insert_with(|| pb::InventoryItem {
                sku_id: request.sku_id.clone(),
                available: 0,
                reserved: 0,
                updated_at: timestamp(OffsetDateTime::now_utc()),
            });
        if request.available < current.reserved {
            return Err(RepositoryError::Insufficient(format!(
                "{} units remain reserved",
                current.reserved
            )));
        }
        current.available = request.available;
        current.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(current.clone())
    }
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, RepositoryError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        state
            .stock
            .get(sku_id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(sku_id.to_string()))
    }
    async fn reserve(
        &self,
        request: pb::ReserveRequest,
    ) -> Result<pb::Reservation, RepositoryError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        if let Some(reservation) = state.reservations.get(&request.reservation_id) {
            return reservation_proto(&request.reservation_id, reservation);
        }
        let quantities = quantities(&request.items)?;
        for (sku_id, quantity) in &quantities {
            let stock = state
                .stock
                .get(sku_id)
                .ok_or_else(|| RepositoryError::NotFound(sku_id.clone()))?;
            if stock.available.saturating_sub(stock.reserved) < *quantity {
                return Err(RepositoryError::Insufficient(sku_id.clone()));
            }
        }
        for (sku_id, quantity) in quantities {
            let stock = state.stock.get_mut(&sku_id).ok_or_else(|| {
                RepositoryError::Failed(format!("stock {sku_id} disappeared during reservation"))
            })?;
            stock.reserved = stock.reserved.saturating_add(quantity);
            stock.updated_at = timestamp(OffsetDateTime::now_utc());
        }
        let reservation = MemoryReservation {
            status: "reserved".to_string(),
            expires_at: OffsetDateTime::now_utc()
                + Duration::seconds(
                    i64::try_from(request.ttl_seconds.unwrap_or(900)).unwrap_or(900),
                ),
            items: request.items,
        };
        let response = reservation_proto(&request.reservation_id, &reservation)?;
        state
            .reservations
            .insert(request.reservation_id, reservation);
        Ok(response)
    }
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, RepositoryError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        let reservation = state
            .reservations
            .get(id)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        if reservation.status == "committed" {
            return reservation_proto(id, &reservation);
        }
        if reservation.status != "reserved" {
            return Err(RepositoryError::Failed(format!(
                "reservation {id} is {}",
                reservation.status
            )));
        }
        for item in &reservation.items {
            let stock = state
                .stock
                .get_mut(&item.sku_id)
                .ok_or_else(|| RepositoryError::NotFound(item.sku_id.clone()))?;
            let quantity = i64::from(item.quantity);
            stock.available = stock.available.saturating_sub(quantity);
            stock.reserved = stock.reserved.saturating_sub(quantity);
            stock.updated_at = timestamp(OffsetDateTime::now_utc());
        }
        let reservation = state
            .reservations
            .get_mut(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        reservation.status = "committed".to_string();
        reservation_proto(id, reservation)
    }
    async fn release(&self, id: &str) -> Result<pb::Reservation, RepositoryError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        let Some(reservation) = state.reservations.get(id).cloned() else {
            return Ok(pb::Reservation {
                id: id.to_string(),
                status: "released".to_string(),
                expires_at: timestamp(OffsetDateTime::now_utc()),
                items: Vec::new(),
            });
        };
        if reservation.status == "released" || reservation.status == "expired" {
            return reservation_proto(id, &reservation);
        }
        if reservation.status != "reserved" {
            return Err(RepositoryError::Failed(format!(
                "reservation {id} is {}",
                reservation.status
            )));
        }
        for item in &reservation.items {
            let stock = state
                .stock
                .get_mut(&item.sku_id)
                .ok_or_else(|| RepositoryError::NotFound(item.sku_id.clone()))?;
            stock.reserved = stock.reserved.saturating_sub(i64::from(item.quantity));
            stock.updated_at = timestamp(OffsetDateTime::now_utc());
        }
        let reservation = state
            .reservations
            .get_mut(id)
            .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        reservation.status = "released".to_string();
        reservation_proto(id, reservation)
    }
    async fn expire(&self, limit: usize) -> Result<u32, RepositoryError> {
        let mut state = self.state.lock().await;
        Ok(expire_memory(&mut state, limit))
    }
}
#[derive(Clone)]
pub(crate) struct PostgresInventoryRepository {
    pool: PgPool,
}
impl PostgresInventoryRepository {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn reservation(&self, id: &str) -> Result<pb::Reservation, RepositoryError> {
        let row = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,status,expires_at FROM mall_inventory_reservations WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        let items = sqlx::query_as::<_, ReservationItemRow>("SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1 ORDER BY sku_id").bind(id).fetch_all(&self.pool).await.map_err(database)?;
        reservation_row(row, items)
    }
}
#[async_trait]
impl InventoryRepository for PostgresInventoryRepository {
    async fn set_stock(
        &self,
        request: pb::SetStockRequest,
    ) -> Result<pb::InventoryItem, RepositoryError> {
        let row = sqlx::query_as::<_, StockRow>("INSERT INTO mall_inventory_stock (sku_id,available) VALUES ($1,$2) ON CONFLICT (sku_id) DO UPDATE SET available=EXCLUDED.available, updated_at=now() WHERE mall_inventory_stock.reserved <= EXCLUDED.available RETURNING sku_id,available,reserved,updated_at").bind(&request.sku_id).bind(request.available).fetch_optional(&self.pool).await.map_err(database)?.ok_or(RepositoryError::Insufficient(request.sku_id))?;
        Ok(stock_proto(row))
    }
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, RepositoryError> {
        let row = sqlx::query_as::<_, StockRow>(
            "SELECT sku_id,available,reserved,updated_at FROM mall_inventory_stock WHERE sku_id=$1",
        )
        .bind(sku_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| RepositoryError::NotFound(sku_id.to_string()))?;
        Ok(stock_proto(row))
    }
    async fn reserve(
        &self,
        request: pb::ReserveRequest,
    ) -> Result<pb::Reservation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        expire_postgres(&mut tx, EXPIRY_SWEEP_LIMIT).await?;
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT status FROM mall_inventory_reservations WHERE id=$1 FOR UPDATE",
        )
        .bind(&request.reservation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if existing.is_some() {
            tx.commit().await.map_err(database)?;
            return self.reservation(&request.reservation_id).await;
        }
        let quantities = quantities(&request.items)?;
        for (sku_id, quantity) in &quantities {
            let changed = sqlx::query("UPDATE mall_inventory_stock SET reserved=reserved+$2, updated_at=now() WHERE sku_id=$1 AND available-reserved >= $2").bind(sku_id).bind(*quantity).execute(&mut *tx).await.map_err(database)?.rows_affected();
            if changed != 1 {
                return Err(RepositoryError::Insufficient(sku_id.clone()));
            }
        }
        let expires_at = OffsetDateTime::now_utc()
            + Duration::seconds(i64::try_from(request.ttl_seconds.unwrap_or(900)).unwrap_or(900));
        sqlx::query("INSERT INTO mall_inventory_reservations (id,status,expires_at) VALUES ($1,'reserved',$2)").bind(&request.reservation_id).bind(expires_at).execute(&mut *tx).await.map_err(database)?;
        for item in &request.items {
            sqlx::query("INSERT INTO mall_inventory_reservation_items (reservation_id,sku_id,quantity) VALUES ($1,$2,$3)").bind(&request.reservation_id).bind(&item.sku_id).bind(i64::from(item.quantity)).execute(&mut *tx).await.map_err(database)?;
        }
        tx.commit().await.map_err(database)?;
        self.reservation(&request.reservation_id).await
    }
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        expire_postgres(&mut tx, EXPIRY_SWEEP_LIMIT).await?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM mall_inventory_reservations WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;
        if status == "committed" {
            tx.commit().await.map_err(database)?;
            return self.reservation(id).await;
        }
        if status != "reserved" {
            tx.commit().await.map_err(database)?;
            return Err(RepositoryError::Failed(format!(
                "reservation {id} is {status}"
            )));
        }
        let items = sqlx::query_as::<_, ReservationItemRow>("SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1 ORDER BY sku_id").bind(id).fetch_all(&mut *tx).await.map_err(database)?;
        for item in items {
            let changed = sqlx::query("UPDATE mall_inventory_stock SET available=available-$2,reserved=reserved-$2,updated_at=now() WHERE sku_id=$1 AND reserved >= $2").bind(item.sku_id).bind(item.quantity).execute(&mut *tx).await.map_err(database)?.rows_affected();
            if changed != 1 {
                return Err(RepositoryError::Failed(
                    "reserved stock no longer matches the reservation".to_string(),
                ));
            }
        }
        sqlx::query("UPDATE mall_inventory_reservations SET status='committed',updated_at=now() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(database)?;
        tx.commit().await.map_err(database)?;
        self.reservation(id).await
    }
    async fn release(&self, id: &str) -> Result<pb::Reservation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM mall_inventory_reservations WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        let Some(status) = status else {
            tx.commit().await.map_err(database)?;
            return Ok(pb::Reservation {
                id: id.to_string(),
                status: "released".to_string(),
                expires_at: timestamp(OffsetDateTime::now_utc()),
                items: Vec::new(),
            });
        };
        if status == "released" || status == "expired" {
            tx.commit().await.map_err(database)?;
            return self.reservation(id).await;
        }
        if status != "reserved" {
            return Err(RepositoryError::Failed(format!(
                "reservation {id} is {status}"
            )));
        }
        let items = sqlx::query_as::<_, ReservationItemRow>(
            "SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .map_err(database)?;
        for item in items {
            let changed = sqlx::query("UPDATE mall_inventory_stock SET reserved=reserved-$2,updated_at=now() WHERE sku_id=$1 AND reserved >= $2").bind(item.sku_id).bind(item.quantity).execute(&mut *tx).await.map_err(database)?.rows_affected();
            if changed != 1 {
                return Err(RepositoryError::Failed(
                    "reserved stock no longer matches the reservation".to_string(),
                ));
            }
        }
        sqlx::query(
            "UPDATE mall_inventory_reservations SET status='released',updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        self.reservation(id).await
    }
    async fn expire(&self, limit: usize) -> Result<u32, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let expired = expire_postgres(&mut transaction, limit).await?;
        transaction.commit().await.map_err(database)?;
        Ok(expired)
    }
}
#[derive(FromRow)]
struct StockRow {
    sku_id: String,
    available: i64,
    reserved: i64,
    updated_at: OffsetDateTime,
}
#[derive(FromRow)]
struct ReservationRow {
    id: String,
    status: String,
    expires_at: OffsetDateTime,
}
#[derive(FromRow)]
struct ReservationItemRow {
    sku_id: String,
    quantity: i64,
}
fn stock_proto(row: StockRow) -> pb::InventoryItem {
    pb::InventoryItem {
        sku_id: row.sku_id,
        available: row.available,
        reserved: row.reserved,
        updated_at: timestamp(row.updated_at),
    }
}
fn reservation_row(
    row: ReservationRow,
    items: Vec<ReservationItemRow>,
) -> Result<pb::Reservation, RepositoryError> {
    Ok(pb::Reservation {
        id: row.id,
        status: row.status,
        expires_at: timestamp(row.expires_at),
        items: items
            .into_iter()
            .map(|item| {
                Ok(pb::ReservationLine {
                    sku_id: item.sku_id,
                    quantity: u32::try_from(item.quantity).map_err(|_| {
                        RepositoryError::Failed("invalid stored reservation quantity".to_string())
                    })?,
                })
            })
            .collect::<Result<_, _>>()?,
    })
}
fn reservation_proto(
    id: &str,
    value: &MemoryReservation,
) -> Result<pb::Reservation, RepositoryError> {
    Ok(pb::Reservation {
        id: id.to_string(),
        status: value.status.clone(),
        expires_at: timestamp(value.expires_at),
        items: value.items.clone(),
    })
}
fn quantities(items: &[pb::ReservationLine]) -> Result<BTreeMap<String, i64>, RepositoryError> {
    let mut values = BTreeMap::new();
    for item in items {
        let entry = values.entry(item.sku_id.clone()).or_insert(0_i64);
        *entry = entry
            .checked_add(i64::from(item.quantity))
            .ok_or_else(|| RepositoryError::Failed("inventory quantity overflow".to_string()))?;
    }
    Ok(values)
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
) -> Result<u32, RepositoryError> {
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
fn database(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Failed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{InventoryRepository, MemoryInventoryRepository, RepositoryError};
    use crate::api::pb;

    #[tokio::test]
    async fn reservation_is_idempotent_and_confirmation_consumes_stock() {
        let repository = MemoryInventoryRepository::default();
        repository
            .set_stock(pb::SetStockRequest {
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
        let first = repository
            .reserve(request.clone())
            .await
            .expect("stock should reserve");
        let retry = repository
            .reserve(request)
            .await
            .expect("same order should return its reservation");
        assert_eq!(first.id, retry.id);
        assert_eq!(
            repository
                .stock("sku-1")
                .await
                .expect("stock exists")
                .reserved,
            2
        );

        let error = repository
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
        assert!(matches!(error, RepositoryError::Insufficient(_)));

        repository
            .confirm("order-1")
            .await
            .expect("reservation should commit");
        let stock = repository.stock("sku-1").await.expect("stock exists");
        assert_eq!(stock.available, 0);
        assert_eq!(stock.reserved, 0);

        let released = repository
            .release("order-without-reservation")
            .await
            .expect("compensation for a failed initial reservation is idempotent");
        assert_eq!(released.status, "released");
    }

    #[tokio::test]
    async fn sweep_releases_expired_reservations_without_a_follow_up_checkout() {
        let repository = MemoryInventoryRepository::default();
        repository
            .set_stock(pb::SetStockRequest {
                sku_id: "sku-1".to_string(),
                available: 1,
            })
            .await
            .expect("stock should be initialized");
        repository
            .reserve(pb::ReserveRequest {
                reservation_id: "order-1".to_string(),
                items: vec![pb::ReservationLine {
                    sku_id: "sku-1".to_string(),
                    quantity: 1,
                }],
                ttl_seconds: Some(0),
            })
            .await
            .expect("stock should reserve");

        assert_eq!(repository.expire(100).await.expect("sweep should work"), 1);
        assert_eq!(
            repository
                .stock("sku-1")
                .await
                .expect("stock should exist")
                .reserved,
            0
        );
    }
}
