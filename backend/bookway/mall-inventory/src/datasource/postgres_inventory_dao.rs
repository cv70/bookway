use super::*;
use sqlx::PgPool;

#[derive(sqlx::FromRow)]
struct StockRow {
    sku_id: String,
    available: i64,
    reserved: i64,
    updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct ReservationRow {
    id: String,
    status: String,
    expires_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
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
) -> Result<pb::Reservation, DaoError> {
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
                        DaoError::Failed("invalid stored reservation quantity".to_string())
                    })?,
                })
            })
            .collect::<Result<_, _>>()?,
    })
}

fn same_stored_reservation_items(
    existing: &[ReservationItemRow],
    requested: &[pb::ReservationLine],
) -> Result<bool, DaoError> {
    let mut stored = BTreeMap::new();
    for item in existing {
        if item.quantity <= 0 {
            return Err(DaoError::Failed(
                "invalid stored reservation quantity".to_string(),
            ));
        }
        let entry = stored.entry(item.sku_id.clone()).or_insert(0_i64);
        *entry = entry
            .checked_add(item.quantity)
            .ok_or_else(|| DaoError::Failed("inventory quantity overflow".to_string()))?;
    }
    Ok(stored == quantities(requested)?)
}

#[derive(Clone)]
pub(crate) struct PostgresInventoryDao {
    pool: PgPool,
}

impl PostgresInventoryDao {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub(super) async fn reservation(&self, id: &str) -> Result<pb::Reservation, DaoError> {
        let row = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,status,expires_at FROM mall_inventory_reservations WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        let items = sqlx::query_as::<_, ReservationItemRow>("SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1 ORDER BY sku_id").bind(id).fetch_all(&self.pool).await.map_err(database)?;
        reservation_row(row, items)
    }
}

#[async_trait]
impl InventoryDao for PostgresInventoryDao {
    async fn set_stock(&self, request: pb::SetStockRequest) -> Result<pb::InventoryItem, DaoError> {
        let row = sqlx::query_as::<_, StockRow>("INSERT INTO mall_inventory_stock (sku_id,available) VALUES ($1,$2) ON CONFLICT (sku_id) DO UPDATE SET available=EXCLUDED.available, updated_at=now() WHERE mall_inventory_stock.reserved <= EXCLUDED.available RETURNING sku_id,available,reserved,updated_at").bind(&request.sku_id).bind(request.available).fetch_optional(&self.pool).await.map_err(database)?.ok_or(DaoError::Insufficient(request.sku_id))?;
        Ok(stock_proto(row))
    }
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, DaoError> {
        let row = sqlx::query_as::<_, StockRow>(
            "SELECT sku_id,available,reserved,updated_at FROM mall_inventory_stock WHERE sku_id=$1",
        )
        .bind(sku_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(sku_id.to_string()))?;
        Ok(stock_proto(row))
    }
    async fn reserve(&self, request: pb::ReserveRequest) -> Result<pb::Reservation, DaoError> {
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
            let existing_items = sqlx::query_as::<_, ReservationItemRow>(
                "SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1",
            )
            .bind(&request.reservation_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(database)?;
            if !same_stored_reservation_items(&existing_items, &request.items)? {
                return Err(DaoError::Conflict(
                    "reservation id is already bound to different inventory items".to_string(),
                ));
            }
            tx.commit().await.map_err(database)?;
            return self.reservation(&request.reservation_id).await;
        }
        let quantities = quantities(&request.items)?;
        for (sku_id, quantity) in &quantities {
            let changed = sqlx::query("UPDATE mall_inventory_stock SET reserved=reserved+$2, updated_at=now() WHERE sku_id=$1 AND available-reserved >= $2").bind(sku_id).bind(*quantity).execute(&mut *tx).await.map_err(database)?.rows_affected();
            if changed != 1 {
                return Err(DaoError::Insufficient(sku_id.clone()));
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
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, DaoError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        expire_postgres(&mut tx, EXPIRY_SWEEP_LIMIT).await?;
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM mall_inventory_reservations WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        if status == "committed" {
            tx.commit().await.map_err(database)?;
            return self.reservation(id).await;
        }
        if status != "reserved" {
            tx.commit().await.map_err(database)?;
            return Err(DaoError::Failed(format!("reservation {id} is {status}")));
        }
        let items = sqlx::query_as::<_, ReservationItemRow>("SELECT sku_id,quantity FROM mall_inventory_reservation_items WHERE reservation_id=$1 ORDER BY sku_id").bind(id).fetch_all(&mut *tx).await.map_err(database)?;
        for item in items {
            let changed = sqlx::query("UPDATE mall_inventory_stock SET available=available-$2,reserved=reserved-$2,updated_at=now() WHERE sku_id=$1 AND reserved >= $2").bind(item.sku_id).bind(item.quantity).execute(&mut *tx).await.map_err(database)?.rows_affected();
            if changed != 1 {
                return Err(DaoError::Failed(
                    "reserved stock no longer matches the reservation".to_string(),
                ));
            }
        }
        sqlx::query("UPDATE mall_inventory_reservations SET status='committed',updated_at=now() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(database)?;
        tx.commit().await.map_err(database)?;
        self.reservation(id).await
    }
    async fn release(&self, id: &str) -> Result<pb::Reservation, DaoError> {
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
            return Err(DaoError::Failed(format!("reservation {id} is {status}")));
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
                return Err(DaoError::Failed(
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
    async fn expire(&self, limit: usize) -> Result<u32, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(database)?;
        let expired = expire_postgres(&mut transaction, limit).await?;
        transaction.commit().await.map_err(database)?;
        Ok(expired)
    }
}
