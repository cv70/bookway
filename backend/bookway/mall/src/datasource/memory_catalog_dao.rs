use super::*;

#[derive(Default)]
pub(crate) struct MemoryCatalogDao {
    products: RwLock<HashMap<String, pb::MallProduct>>,
    product_merchants: RwLock<HashMap<String, String>>,
    node_offers: RwLock<HashMap<String, pb::NodeOffer>>,
    node_offer_idempotency: RwLock<HashMap<String, String>>,
}

#[async_trait]
impl CatalogDao for MemoryCatalogDao {
    async fn create(&self, request: pb::CreateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let merchant_id = request.merchant_id.clone();
        let product = new_product(request);
        self.products
            .write()
            .await
            .insert(product.id.clone(), product.clone());
        self.product_merchants
            .write()
            .await
            .insert(product.id.clone(), merchant_id);
        Ok(product)
    }
    async fn update(&self, request: pb::UpdateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let mut products = self.products.write().await;
        let product = products
            .get_mut(&request.product_id)
            .ok_or_else(|| DaoError::NotFound(request.product_id.clone()))?;
        let owner = self
            .product_merchants
            .read()
            .await
            .get(&request.product_id)
            .cloned();
        if owner.as_deref() != Some(request.merchant_id.as_str()) {
            return Err(DaoError::NotFound(request.product_id));
        }
        apply_update(product, request)?;
        Ok(product.clone())
    }
    async fn list(&self, request: pb::ProductQueryRequest) -> Result<pb::ProductPage, DaoError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 100)).unwrap_or(100);
        let cursor = request.cursor.unwrap_or_default();
        let query = request.query.unwrap_or_default().to_lowercase();
        let merchant_id = request.merchant_id.unwrap_or_default();
        let product_merchants = self.product_merchants.read().await.clone();
        let mut values = self
            .products
            .read()
            .await
            .values()
            .filter(|product| {
                request.include_inactive || product.status == pb::MallProductStatus::Active as i32
            })
            .filter(|product| {
                request.include_inactive || product.skus.iter().any(|sku| sku.saleable)
            })
            .filter(|product| {
                merchant_id.is_empty()
                    || product_merchants
                        .get(&product.id)
                        .is_some_and(|owner| owner == &merchant_id)
            })
            .filter(|product| product.id > cursor)
            .filter(|product| query.is_empty() || product.title.to_lowercase().contains(&query))
            .cloned()
            .map(|product| {
                if request.include_inactive {
                    product
                } else {
                    customer_product(&product)
                }
            })
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        let next_cursor = if values.len() > limit {
            Some(values[limit - 1].id.clone())
        } else {
            None
        };
        values.truncate(limit);
        Ok(pb::ProductPage {
            items: values,
            next_cursor,
        })
    }
    async fn get(&self, id: &str) -> Result<pb::MallProduct, DaoError> {
        self.products
            .read()
            .await
            .get(id)
            .cloned()
            .filter(|product| {
                product.status == pb::MallProductStatus::Active as i32
                    && product.skus.iter().any(|sku| sku.saleable)
            })
            .map(|product| customer_product(&product))
            .ok_or_else(|| DaoError::NotFound(id.to_string()))
    }
    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, DaoError> {
        let ids = ids.into_iter().collect::<std::collections::HashSet<_>>();
        let mut values = self
            .products
            .read()
            .await
            .values()
            .filter(|product| product.status == pb::MallProductStatus::Active as i32)
            .flat_map(|product| product.skus.iter())
            .filter(|sku| sku.saleable && ids.contains(&sku.id))
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(values)
    }
    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, DaoError> {
        let owner = self
            .product_merchants
            .read()
            .await
            .get(&request.product_id)
            .cloned();
        if owner.as_deref() != Some(request.merchant_id.as_str()) {
            return Err(DaoError::NotFound(request.product_id));
        }
        let mut idempotency = self.node_offer_idempotency.write().await;
        if let Some(id) = idempotency.get(&request.idempotency_key) {
            let existing = self
                .node_offers
                .read()
                .await
                .get(id)
                .cloned()
                .ok_or_else(|| {
                    DaoError::Failed("missing node offer idempotency target".to_string())
                })?;
            if !offer_matches_request(&existing, &request) {
                return Err(DaoError::Conflict(
                    "idempotency key is already bound to a different node offer".to_string(),
                ));
            }
            return Ok(existing);
        }
        let offer = pb::NodeOffer {
            id: Uuid::now_v7().to_string(),
            product_id: request.product_id,
            sku_id: request.sku_id,
            route_id: request.route_id,
            action_node_id: request.action_node_id,
            creator_id: request.creator_id,
            commission_bps: request.commission_bps,
            created_at: timestamp(OffsetDateTime::now_utc()),
            scene_equipment: request.scene_equipment,
            product: None,
            merchant_id: request.merchant_id,
        };
        idempotency.insert(request.idempotency_key, offer.id.clone());
        self.node_offers
            .write()
            .await
            .insert(offer.id.clone(), offer.clone());
        Ok(offer)
    }
    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, DaoError> {
        let limit = usize::try_from(request.limit.unwrap_or(20).clamp(1, 50)).unwrap_or(50);
        let products = self.products.read().await;
        let mut offers = self
            .node_offers
            .read()
            .await
            .values()
            .filter(|offer| offer.route_id == request.route_id)
            .filter(|offer| offer.action_node_id == request.action_node_id)
            .filter(|offer| offer.scene_equipment == request.scene_equipment)
            .filter_map(|offer| {
                let product = products.get(&offer.product_id)?;
                let saleable = product.status == pb::MallProductStatus::Active as i32
                    && product
                        .skus
                        .iter()
                        .any(|sku| sku.id == offer.sku_id && sku.saleable);
                saleable.then(|| {
                    let mut offer = offer.clone();
                    offer.product = Some(customer_product(product));
                    offer
                })
            })
            .collect::<Vec<_>>();
        offers.sort_by(|left, right| left.id.cmp(&right.id));
        offers.truncate(limit);
        Ok(offers)
    }
    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, DaoError> {
        self.node_offers
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| DaoError::NotFound(id.to_string()))
    }
}

fn customer_product(product: &pb::MallProduct) -> pb::MallProduct {
    let mut product = product.clone();
    product.skus.retain(|sku| sku.saleable);
    product
}
