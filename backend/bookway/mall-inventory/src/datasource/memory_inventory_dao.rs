use super::*;

#[derive(Default)]
pub(crate) struct MemoryInventoryDao {
    state: Mutex<MemoryState>,
}

#[async_trait]
impl InventoryDao for MemoryInventoryDao {
    async fn set_stock(&self, request: pb::SetStockRequest) -> Result<pb::InventoryItem, DaoError> {
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
            return Err(DaoError::Insufficient(format!(
                "{} units remain reserved",
                current.reserved
            )));
        }
        current.available = request.available;
        current.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(current.clone())
    }
    async fn stock(&self, sku_id: &str) -> Result<pb::InventoryItem, DaoError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        state
            .stock
            .get(sku_id)
            .cloned()
            .ok_or_else(|| DaoError::NotFound(sku_id.to_string()))
    }
    async fn reserve(&self, request: pb::ReserveRequest) -> Result<pb::Reservation, DaoError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        if let Some(reservation) = state.reservations.get(&request.reservation_id) {
            if !same_reservation_items(&reservation.items, &request.items)? {
                return Err(DaoError::Conflict(
                    "reservation id is already bound to different inventory items".to_string(),
                ));
            }
            return reservation_proto(&request.reservation_id, reservation);
        }
        let quantities = quantities(&request.items)?;
        for (sku_id, quantity) in &quantities {
            let stock = state
                .stock
                .get(sku_id)
                .ok_or_else(|| DaoError::NotFound(sku_id.clone()))?;
            if stock.available.saturating_sub(stock.reserved) < *quantity {
                return Err(DaoError::Insufficient(sku_id.clone()));
            }
        }
        for (sku_id, quantity) in quantities {
            let stock = state.stock.get_mut(&sku_id).ok_or_else(|| {
                DaoError::Failed(format!("stock {sku_id} disappeared during reservation"))
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
    async fn confirm(&self, id: &str) -> Result<pb::Reservation, DaoError> {
        let mut state = self.state.lock().await;
        expire_memory(&mut state, EXPIRY_SWEEP_LIMIT);
        let reservation = state
            .reservations
            .get(id)
            .cloned()
            .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        if reservation.status == "committed" {
            return reservation_proto(id, &reservation);
        }
        if reservation.status != "reserved" {
            return Err(DaoError::Failed(format!(
                "reservation {id} is {}",
                reservation.status
            )));
        }
        for item in &reservation.items {
            let stock = state
                .stock
                .get_mut(&item.sku_id)
                .ok_or_else(|| DaoError::NotFound(item.sku_id.clone()))?;
            let quantity = i64::from(item.quantity);
            stock.available = stock.available.saturating_sub(quantity);
            stock.reserved = stock.reserved.saturating_sub(quantity);
            stock.updated_at = timestamp(OffsetDateTime::now_utc());
        }
        let reservation = state
            .reservations
            .get_mut(id)
            .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        reservation.status = "committed".to_string();
        reservation_proto(id, reservation)
    }
    async fn release(&self, id: &str) -> Result<pb::Reservation, DaoError> {
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
        if reservation.status == "released"
            || reservation.status == "expired"
            || reservation.status == "committed"
        {
            return reservation_proto(id, &reservation);
        }
        if reservation.status != "reserved" {
            return Err(DaoError::Failed(format!(
                "reservation {id} is {}",
                reservation.status
            )));
        }
        for item in &reservation.items {
            let stock = state
                .stock
                .get_mut(&item.sku_id)
                .ok_or_else(|| DaoError::NotFound(item.sku_id.clone()))?;
            stock.reserved = stock.reserved.saturating_sub(i64::from(item.quantity));
            stock.updated_at = timestamp(OffsetDateTime::now_utc());
        }
        let reservation = state
            .reservations
            .get_mut(id)
            .ok_or_else(|| DaoError::NotFound(id.to_string()))?;
        reservation.status = "released".to_string();
        reservation_proto(id, reservation)
    }
    async fn expire(&self, limit: usize) -> Result<u32, DaoError> {
        let mut state = self.state.lock().await;
        Ok(expire_memory(&mut state, limit))
    }
}
