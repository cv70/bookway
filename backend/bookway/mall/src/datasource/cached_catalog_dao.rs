use super::*;

use std::{future::Future, sync::Arc};

use async_trait::async_trait;
use redis::aio::ConnectionManager;

use crate::api::pb;

/// Serialization carrier for [`CatalogDao::skus`], which hands out a bare
/// sequence rather than a message the versioned cache can frame.
#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct SkuList {
    #[prost(message, repeated, tag = "1")]
    pub(crate) skus: Vec<pb::MallSku>,
}

/// Read-through cache for the two public catalog hot reads (node offers per
/// scene and SKU batches). Mutations bump one catalog-wide counter per cache,
/// which retires every stamped entry immediately — merchant edits land right
/// away, and the cost is one INCR per write on an overwhelmingly read-heavy
/// workload. Checkout still resolves offers by ID straight from the source of
/// truth; stock gating lives in mall-inventory's own Redis+Lua path.
pub(crate) struct CachedCatalogDao {
    inner: Arc<dyn CatalogDao>,
    offers: bookway_cache::VersionedCache<pb::NodeOfferList>,
    skus: bookway_cache::VersionedCache<SkuList>,
}

const OFFER_TTL_SECONDS: u64 = 20;
const SKU_TTL_SECONDS: u64 = 20;
// Must outlive both payload TTLs; see `VersionedCache::new_scoped`.
const VERSION_TTL_SECONDS: u64 = 120;
const CATALOG_SCOPE: &str = "catalog";

impl CachedCatalogDao {
    pub(crate) fn new(inner: Arc<dyn CatalogDao>, redis: Option<ConnectionManager>) -> Self {
        let offers = bookway_cache::VersionedCache::new(
            redis.clone(),
            "bookway:mall:offers",
            OFFER_TTL_SECONDS,
            VERSION_TTL_SECONDS,
        );
        let skus = bookway_cache::VersionedCache::new(
            redis,
            "bookway:mall:skus",
            SKU_TTL_SECONDS,
            VERSION_TTL_SECONDS,
        );
        Self { inner, offers, skus }
    }

    async fn invalidate_catalog(&self) {
        self.offers.invalidate(CATALOG_SCOPE).await;
        self.skus.invalidate(CATALOG_SCOPE).await;
    }
}

#[async_trait]
impl CatalogDao for CachedCatalogDao {
    async fn create(&self, request: pb::CreateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let product = self.inner.create(request).await?;
        self.invalidate_catalog().await;
        Ok(product)
    }

    async fn update(&self, request: pb::UpdateProductRequest) -> Result<pb::MallProduct, DaoError> {
        let product = self.inner.update(request).await?;
        self.invalidate_catalog().await;
        Ok(product)
    }

    async fn list(&self, request: pb::ProductQueryRequest) -> Result<pb::ProductPage, DaoError> {
        self.inner.list(request).await
    }

    async fn get(&self, id: &str) -> Result<pb::MallProduct, DaoError> {
        self.inner.get(id).await
    }

    async fn skus(&self, ids: Vec<String>) -> Result<Vec<pb::MallSku>, DaoError> {
        let mut canonical = ids;
        canonical.sort_unstable();
        canonical.dedup();
        canonical.retain(|id| !id.trim().is_empty());
        if canonical.is_empty() {
            return Ok(Vec::new());
        }
        let entry = canonical.join("\u{1}");
        let cached = cached_message(
            &self.skus,
            &entry,
            CATALOG_SCOPE,
            async {
                self.inner
                    .skus(canonical.clone())
                    .await
                    .map(|skus| SkuList { skus })
            },
        )
        .await?;
        Ok(cached.skus)
    }

    async fn attach_node_offer(
        &self,
        request: pb::AttachNodeOfferRequest,
    ) -> Result<pb::NodeOffer, DaoError> {
        let offer = self.inner.attach_node_offer(request).await?;
        self.offers.invalidate(CATALOG_SCOPE).await;
        Ok(offer)
    }

    async fn node_offers(
        &self,
        request: pb::NodeOfferQueryRequest,
    ) -> Result<Vec<pb::NodeOffer>, DaoError> {
        // The domain layer normalizes these fields before the dao call;
        // normalize again so a future caller cannot fragment the key space.
        let entry = format!(
            "{}\u{1}{}\u{1}{}",
            request.route_id.trim(),
            request.action_node_id.trim(),
            request.scene_equipment.trim()
        );
        let cached = cached_message(
            &self.offers,
            &entry,
            CATALOG_SCOPE,
            async {
                self.inner
                    .node_offers(request)
                    .await
                    .map(|items| pb::NodeOfferList { items })
            },
        )
        .await?;
        Ok(cached.items)
    }

    async fn node_offer(&self, id: &str) -> Result<pb::NodeOffer, DaoError> {
        self.inner.node_offer(id).await
    }
    async fn verify_merchant_sku(
        &self,
        request: pb::MerchantSkuRequest,
    ) -> Result<pb::MerchantSkuDecision, DaoError> {
        // An ownership verdict authorizes a mutation; serve it from the
        // source of truth rather than any cache layer.
        self.inner.verify_merchant_sku(request).await
    }
}

/// Versioned read-through with miss single-flight: serves only payloads whose
/// stamp matches the live counter, and stamps a pre-reload snapshot so an
/// invalidation racing the rebuild retires whatever gets stored.
async fn cached_message<M>(
    cache: &bookway_cache::VersionedCache<M>,
    entry: &str,
    scope: &str,
    load: impl Future<Output = Result<M, DaoError>>,
) -> Result<M, DaoError>
where
    M: prost::Message + Default + Clone,
{
    if let Some(value) = cache.load(entry, scope).await {
        return Ok(value);
    }

    let guard = cache.refresh_lock(entry).await;
    if let Some(value) = cache.load(entry, scope).await {
        guard.release().await;
        return Ok(value);
    }

    let version = cache.version(scope).await;
    let result = load.await;
    if let (Some(version), Ok(value)) = (version, result.as_ref()) {
        cache.store(entry, version, value).await;
    }
    guard.release().await;
    result
}

#[cfg(test)]
mod tests {
    use super::{CachedCatalogDao, CatalogDao, MemoryCatalogDao};
    use crate::api::pb;

    /// Without Redis the versioned caches degrade to pass-through reads: a
    /// mutation followed immediately by a public read must never serve a
    /// shadow of the previous catalog.
    #[tokio::test]
    async fn mutations_are_visible_to_public_reads_without_redis() {
        let dao = CachedCatalogDao::new(std::sync::Arc::new(MemoryCatalogDao::default()), None);
        let product = dao
            .create(pb::CreateProductRequest {
                merchant_id: "merchant-a".to_string(),
                title: "Trail kit".to_string(),
                description: String::new(),
                image_url: String::new(),
                status: pb::MallProductStatus::Active as i32,
                product_kind: pb::MallProductKind::Physical as i32,
                course_resource_id: String::new(),
                skus: vec![pb::CreateSkuRequest {
                    title: "Standard".to_string(),
                    price_cents: 1_000,
                    currency: "CNY".to_string(),
                    attributes: Default::default(),
                    saleable: true,
                }],
            })
            .await
            .expect("product should be created");

        // The mutation itself invalidates both scopes; a subsequent hot read
        // reflects it in the same request cycle.
        dao.attach_node_offer(pb::AttachNodeOfferRequest {
            merchant_id: "merchant-a".to_string(),
            product_id: product.id.clone(),
            sku_id: product.skus[0].id.clone(),
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            creator_id: "creator-1".to_string(),
            commission_bps: 500,
            idempotency_key: "offer-key".to_string(),
            scene_equipment: "trail-running shoes".to_string(),
        })
        .await
        .expect("offer should be attached");

        let offers = dao
            .node_offers(pb::NodeOfferQueryRequest {
                route_id: "route-1".to_string(),
                action_node_id: "node-1".to_string(),
                limit: Some(10),
                scene_equipment: "trail-running shoes".to_string(),
            })
            .await
            .expect("offers should be readable after attach");
        assert_eq!(offers.len(), 1);

        let skus = dao
            .skus(vec![product.skus[0].id.clone(), product.skus[0].id.clone()])
            .await
            .expect("sku batch should resolve");
        // Duplicated ids in one batch collapse to a single SKU.
        assert_eq!(skus.len(), 1);
    }
}
