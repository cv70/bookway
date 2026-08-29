#![allow(clippy::module_inception)]

mod domain;
pub(crate) mod pipeline;

use self::pipeline::{FeedPipeline, ServedFeed};
use super::api::pb;
use bookway_ad_main_api::pb::{self as ad_pb, ad_main_client::AdMainClient};
use bookway_commercial_mix::{MixPolicy, MixedItem};
use prost::Message;
use std::sync::Arc;

/// Low-density contextual commerce for the action feed: at most ~10% of a
/// full page may be commercial, never before the opening three organics.
/// The same schedule caps how many decisions are requested upstream so
/// supply never exceeds what the page can legitimately render.
const FEED_AD_POLICY: MixPolicy = MixPolicy::new(1_000, 3);

#[derive(Clone)]
pub(crate) struct FeedService {
    pipeline: FeedPipeline,
    ad_main: AdMainClient<tonic::transport::Channel>,
    page_cache: Option<Arc<bookway_cache::SingleFlightCache<CachedFeedPage>>>,
}
pub use domain::Domain;

impl FeedService {
    pub(crate) fn new(
        pipeline: FeedPipeline,
        ad_main: AdMainClient<tonic::transport::Channel>,
        page_cache: Option<Arc<bookway_cache::SingleFlightCache<CachedFeedPage>>>,
    ) -> Self {
        Self {
            pipeline,
            ad_main,
            page_cache,
        }
    }

    pub(crate) async fn recommend(&self, request: pb::FeedRequest) -> pb::FeedResponse {
        let action_context = request.action_context.clone();
        let limit = request.limit.unwrap_or(20).clamp(1, 100) as usize;
        let page_key = cacheable_cold_start_page(&request).then(|| cold_start_page_key(&request));
        let mut served = match (&self.page_cache, &page_key) {
            (Some(cache), Some(key)) => ServedFeed {
                response: self.cold_start_page(&request, cache, key).await,
                // Cold-start pages are anonymous-only by the cacheable check:
                // there is no exposure row and no guard increment to attach.
                exposure: None,
                rendered_ids: Vec::new(),
                geo_region: request.geo_region.clone(),
                device_os: request.device_os.clone(),
            },
            _ => self.pipeline.execute(request.clone()).await,
        };
        let (geo_region, device_os) = (served.geo_region.clone(), served.device_os.clone());
        let Some(context) = action_context.filter(valid_action_context) else {
            self.persist_served(&mut served).await;
            return served.response;
        };
        // Ads require a user-scoped frequency decision. Anonymous contextual
        // feeds remain organic-only instead of making an ad RPC that can only
        // fail on its missing identity and mark the useful response degraded.
        if request.user_id.trim().is_empty() {
            self.persist_served(&mut served).await;
            return served.response;
        }
        // Skip the decision RPC entirely when the mix schedule offers no slot
        // (short pages, tiny limits) or the recall is too thin to guarantee a
        // useful organic experience.
        let ad_slots = FEED_AD_POLICY.ad_slots_for(limit);
        if ad_slots == 0 || served.response.items.len() < FEED_AD_POLICY.min_natural_results {
            self.persist_served(&mut served).await;
            return served.response;
        }

        let ad_request = match service_request(ad_pb::DecisionRequest {
            user_id: request.user_id,
            placement: context.placement.clone(),
            domain: context.domain.clone(),
            limit: Some(u32::try_from(ad_slots).unwrap_or(1)),
            route_id: context.route_id.clone(),
            action_node_id: context.action_node_id.clone(),
            scene_equipment: context.scene_equipment.clone(),
            // Edge-derived delivery context; empty values fail closed to
            // unrestricted campaigns only (ad-center matching rule).
            geo_region,
            device_os,
        }) {
            Ok(request) => request,
            Err(error) => {
                tracing::warn!(%error, "contextual ad request setup degraded; serving organic feed");
                if let Some(meta) = &mut served.response.meta {
                    meta.degraded = true;
                }
                self.persist_served(&mut served).await;
                return served.response;
            }
        };
        let mut ad_main = self.ad_main.clone();
        match ad_main.decide(ad_request).await {
            Ok(decision) => {
                mix_contextual_ad(&mut served.response, decision.into_inner(), &context, limit)
            }
            Err(error) => {
                tracing::warn!(%error, "contextual ad decision degraded; serving organic feed");
                if let Some(meta) = &mut served.response.meta {
                    meta.degraded = true;
                }
            }
        }
        self.persist_served(&mut served).await;
        served.response
    }

    /// Exposure persistence runs ONCE per request, after commercial mixing:
    /// displaced organics are cut from the ledger and the frequency guard,
    /// which otherwise learn "served" facts about content that never rendered.
    async fn persist_served(&self, served: &mut ServedFeed) {
        served.rendered_ids = served
            .response
            .items
            .iter()
            .filter_map(|item| item.post.as_ref().map(|post| post.id.clone()))
            .collect();
        if self.pipeline.persist(served).await
            && let Some(meta) = &mut served.response.meta
        {
            meta.degraded = true;
        }
    }
}

fn valid_action_context(context: &pb::FeedActionContext) -> bool {
    !context.route_id.trim().is_empty()
        && !context.action_node_id.trim().is_empty()
        && !context.placement.trim().is_empty()
        && context
            .scene_equipment
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

/// Cold-start pages carry no signal of their own: no identity, no cursor
/// history, no declared interests and no action context. They are the only
/// feed surfaces whose natural ranking is identical for every caller within a
/// short window, so a shared page snapshot is safe to reuse. Any
/// personalization input opts the request out entirely.
fn cacheable_cold_start_page(request: &pb::FeedRequest) -> bool {
    request.user_id.trim().is_empty()
        && request.cursor.is_none()
        && request.seen.is_empty()
        && request.interests.is_empty()
        && request.action_context.is_none()
}

fn cold_start_page_key(request: &pb::FeedRequest) -> String {
    let surface = request.surface.trim();
    let surface = if surface.is_empty() { "home" } else { surface };
    format!("{surface}|{}", request.limit.unwrap_or(20).clamp(1, 100))
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CachedFeedPage {
    encoded: Vec<u8>,
}

impl CachedFeedPage {
    fn from_response(response: &pb::FeedResponse) -> Self {
        Self {
            encoded: response.encode_to_vec(),
        }
    }

    fn into_response(self) -> Option<pb::FeedResponse> {
        pb::FeedResponse::decode(self.encoded.as_slice()).ok()
    }
}

impl FeedService {
    /// Read-through page cache for cold-start home pages. A peer instance that
    /// already holds the rebuild lease does not stop this instance from
    /// building its own page — serving an empty window is never acceptable.
    async fn cold_start_page(
        &self,
        request: &pb::FeedRequest,
        cache: &bookway_cache::SingleFlightCache<CachedFeedPage>,
        key: &str,
    ) -> pb::FeedResponse {
        if let Some(page) = cache
            .load(key)
            .await
            .and_then(CachedFeedPage::into_response)
        {
            return page;
        }
        let guard = cache.refresh_lock(key).await;
        let page = if guard.peer_holds_lease() {
            self.pipeline.execute(request.clone()).await.response
        } else {
            match cache.load(key).await.and_then(CachedFeedPage::into_response) {
                Some(page) => {
                    guard.release().await;
                    return page;
                }
                None => self.pipeline.execute(request.clone()).await.response,
            }
        };
        cache
            .store(key, &CachedFeedPage::from_response(&page))
            .await;
        guard.release().await;
        page
    }
}

/// Interleaves auction-ordered contextual ads into the organic page using the
/// shared commercial-mix schedule. Decision items that do not exactly bind to
/// the requested route, node, placement and scene equipment are dropped
/// before the mix, so a mismatched campaign can never buy its way in.
///
/// Naturals displaced past the page limit are dropped here. The caller persists
/// exposure only AFTER this mixing, so a displaced item never enters the ledger
/// or the frequency guard — but the response cursor was already advanced past it
/// by recall, so it is skipped for good rather than deferred to the next page.
/// The loss is bounded by the ad slot count and always falls on the lowest-ranked
/// tail of the page. Search does defer its overflow (`session.pending`), which it
/// can because it owns a session pending list; the feed's cursor is recall's
/// opaque token and cannot be rewound. Fixing this properly means reserving ad
/// slots before selection, which would shrink every page whose ad decision comes
/// back empty — a worse trade at current fill rates.
fn mix_contextual_ad(
    response: &mut pb::FeedResponse,
    decision: ad_pb::DecisionResponse,
    context: &pb::FeedActionContext,
    limit: usize,
) {
    let ad_pb::DecisionResponse {
        items, degraded, ..
    } = decision;
    let ads: Vec<pb::FeedAd> = items
        .into_iter()
        .filter(|ad| {
            ad.route_id == context.route_id
                && ad.action_node_id == context.action_node_id
                && ad.placement == context.placement
                && scene_equipment_key(&ad.scene_equipment)
                    == scene_equipment_key(context.scene_equipment.as_deref().unwrap_or_default())
        })
        .map(|ad| pb::FeedAd {
            request_id: ad.request_id,
            campaign_id: ad.campaign_id,
            placement: ad.placement,
            title: ad.title,
            body: ad.body,
            image_url: ad.image_url,
            landing_url: ad.landing_url,
            ecpm: ad.ecpm,
            model_version: ad.model_version,
            route_id: ad.route_id,
            action_node_id: ad.action_node_id,
            scene_equipment: ad.scene_equipment,
        })
        .collect();
    if ads.is_empty() {
        if degraded && let Some(meta) = &mut response.meta {
            meta.degraded = true;
        }
        return;
    }
    let organics = std::mem::take(&mut response.items);
    let (mixed, overflow) =
        bookway_commercial_mix::mix_page(organics, ads, limit, FEED_AD_POLICY);
    // Displaced tail organics stay invisible for this response and are never
    // recorded as served (exposure persistence runs after this function and
    // filters by rendered ids). They are not re-offered later, though — see the
    // note on this function.
    drop(overflow);
    let mut items = Vec::with_capacity(mixed.len());
    for slot in mixed {
        match slot {
            MixedItem::Organic(item) => items.push(item),
            MixedItem::Ad(ad) => items.push(pb::FeedItem {
                author_id: String::new(),
                post: None,
                score: ad.ecpm,
                source: "contextual_ad_ecpm".to_string(),
                reasons: vec![
                    "action_node_context".to_string(),
                    "ecpm_auction".to_string(),
                ],
                ad: Some(ad),
            }),
        }
    }
    response.items = items;
    // Refresh the rendered count so clients can reconcile the actual page
    // (including the ads) without treating ads as organic candidates.
    if let Some(meta) = &mut response.meta {
        meta.selected = u32::try_from(response.items.len()).unwrap_or(u32::MAX);
    }
    if degraded && let Some(meta) = &mut response.meta {
        meta.degraded = true;
    }
}

fn scene_equipment_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn service_request<T>(
    value: T,
) -> Result<tonic::Request<T>, bookway_runtime::GrpcServiceAuthError> {
    bookway_runtime::grpc_service_request(value)
}

#[cfg(test)]
mod tests {
    use super::{
        CachedFeedPage, cacheable_cold_start_page, cold_start_page_key, mix_contextual_ad,
        valid_action_context,
    };
    use crate::api::pb;
    use bookway_ad_main_api::pb as ad_pb;

    fn anonymous_request() -> pb::FeedRequest {
        pb::FeedRequest {
            user_id: String::new(),
            surface: "home".to_string(),
            limit: Some(20),
            ..Default::default()
        }
    }

    #[test]
    fn only_signal_free_first_pages_are_cacheable() {
        assert!(cacheable_cold_start_page(&anonymous_request()));

        // Any personalization signal opts the page out of the shared cache.
        let personalized = pb::FeedRequest {
            user_id: "user-1".to_string(),
            ..anonymous_request()
        };
        assert!(!cacheable_cold_start_page(&personalized));

        let paginated = pb::FeedRequest {
            cursor: Some("page-2".to_string()),
            ..anonymous_request()
        };
        assert!(!cacheable_cold_start_page(&paginated));

        let history = pb::FeedRequest {
            seen: vec!["post-1".to_string()],
            ..anonymous_request()
        };
        assert!(!cacheable_cold_start_page(&history));

        let declared_interests = pb::FeedRequest {
            interests: vec![0],
            ..anonymous_request()
        };
        assert!(!cacheable_cold_start_page(&declared_interests));

        let contextual = pb::FeedRequest {
            action_context: Some(context()),
            ..anonymous_request()
        };
        assert!(!cacheable_cold_start_page(&contextual));
    }

    #[test]
    fn cold_start_pages_share_one_key_per_surface_and_size() {
        let mut home = anonymous_request();
        home.session_id = "session-a".to_string();
        let other_home = anonymous_request();
        assert_eq!(cold_start_page_key(&home), cold_start_page_key(&other_home));
        assert_eq!(cold_start_page_key(&other_home), "home|20");

        let following = pb::FeedRequest {
            surface: "following".to_string(),
            limit: None,
            ..anonymous_request()
        };
        assert_eq!(cold_start_page_key(&following), "following|20");
    }

    #[test]
    fn cached_page_roundtrip_preserves_the_response() {
        let mut response = pb::FeedResponse::default();
        response.items.push(organic("post-1"));
        response.items.push(organic("post-2"));
        response.meta = Some(pb::FeedMeta {
            selected: 2,
            sourced: 5,
            filtered: 3,
            pipeline_id: "pipeline-v1".to_string(),
            degraded: true,
            model_version: Some("model-v1".to_string()),
            experiment_bucket: Some("bucket-a".to_string()),
            next_cursor: Some("cursor-1".to_string()),
        });

        let restored = CachedFeedPage::from_response(&response)
            .into_response()
            .expect("protobuf-encoded page decodes");
        assert_eq!(restored.items.len(), 2);
        assert_eq!(restored.items[1].author_id, "post-2");
        let meta = restored.meta.expect("meta survives the roundtrip");
        assert_eq!(meta.pipeline_id, "pipeline-v1");
        assert!(meta.degraded);
        assert_eq!(meta.next_cursor.as_deref(), Some("cursor-1"));
    }

    fn context() -> pb::FeedActionContext {
        pb::FeedActionContext {
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            placement: "route_feed".to_string(),
            domain: Some("movement".to_string()),
            scene_equipment: Some("trail shoes".to_string()),
        }
    }

    fn organic(id: &str) -> pb::FeedItem {
        pb::FeedItem {
            author_id: id.to_string(),
            post: None,
            score: 0.8,
            source: "organic".to_string(),
            reasons: Vec::new(),
            ad: None,
        }
    }

    /// A full default page is the regime where the mix schedule actually
    /// offers slots; short pages are covered by the guard tests above.
    fn full_page() -> pb::FeedResponse {
        pb::FeedResponse {
            request_id: "feed-1".to_string(),
            items: (0..20).map(|index| organic(&index.to_string())).collect(),
            meta: Some(pb::FeedMeta {
                sourced: 20,
                filtered: 0,
                selected: 20,
                next_cursor: None,
                pipeline_id: "test".to_string(),
                degraded: false,
                model_version: None,
                experiment_bucket: None,
            }),
        }
    }

    fn feed_ad(campaign: &str, equipment: &str, ecpm: f64) -> ad_pb::AdDecision {
        ad_pb::AdDecision {
            request_id: "ad-request-1".to_string(),
            campaign_id: campaign.to_string(),
            placement: "route_feed".to_string(),
            title: "Trail shoes".to_string(),
            body: String::new(),
            image_url: String::new(),
            landing_url: String::new(),
            score: ecpm,
            model_version: "ecpm-v1".to_string(),
            route_id: "route-1".to_string(),
            action_node_id: "node-1".to_string(),
            scene_equipment: equipment.to_string(),
            ecpm,
        }
    }

    fn ad_positions(response: &pb::FeedResponse) -> Vec<usize> {
        response
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.ad.is_some().then_some(index))
            .collect()
    }

    #[test]
    fn contextual_ads_are_low_density_auction_ordered_and_bound_to_their_node() {
        let mut response = full_page();
        let high = feed_ad("campaign-high", "trail shoes", 42.0);
        let low = feed_ad("campaign-low", "trail shoes", 20.0);
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                request_id: "ad-request-1".to_string(),
                items: vec![high, low],
                degraded: false,
            },
            &context(),
            20,
        );

        assert_eq!(response.items.len(), 20);
        let positions = ad_positions(&response);
        assert_eq!(positions.len(), 2);
        assert!(!response.items[0].ad.is_some());
        // The schedule keeps commerce out of the opening quarter.
        assert!(positions[0] >= 3, "slots: {positions:?}");
        assert!(positions[1] > positions[0]);
        // Strongest inventory wins the earliest slot.
        assert_eq!(
            response.items[positions[0]].ad.as_ref().map(|ad| ad.campaign_id.as_str()),
            Some("campaign-high")
        );
        assert_eq!(
            response.items[positions[1]].ad.as_ref().map(|ad| ad.campaign_id.as_str()),
            Some("campaign-low")
        );
    }

    #[test]
    fn short_pages_never_request_or_render_ads() {
        // 5 items on a limit-5 page: below the schedule's minimum depth for a
        // first slot, so the page stays commercial-free at full length.
        let mut response = pb::FeedResponse {
            items: (0..5).map(|index| organic(&index.to_string())).collect(),
            ..full_page()
        };
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                request_id: "ad-request-2".to_string(),
                items: vec![feed_ad("campaign-x", "trail shoes", 42.0)],
                degraded: false,
            },
            &context(),
            5,
        );
        assert_eq!(response.items.len(), 5);
        assert!(response.items.iter().all(|item| item.ad.is_none()));
    }

    #[test]
    fn incomplete_action_context_never_requests_ads() {
        let mut invalid = context();
        invalid.action_node_id.clear();
        assert!(!valid_action_context(&invalid));
    }

    #[test]
    fn equipment_mismatch_never_enters_the_contextual_slots() {
        let mut response = full_page();
        let decision = ad_pb::DecisionResponse {
            request_id: "ad-request-2".to_string(),
            items: vec![feed_ad("campaign-2", "different equipment", 42.0)],
            degraded: false,
        };
        mix_contextual_ad(&mut response, decision.clone(), &context(), 20);
        assert!(response.items.iter().all(|item| item.ad.is_none()));
        assert_eq!(response.items.len(), 20);

        let matched = ad_pb::DecisionResponse {
            request_id: "ad-request-2".to_string(),
            items: vec![ad_pb::AdDecision {
                scene_equipment: "trail shoes".to_string(),
                ..decision.items[0].clone()
            }],
            degraded: false,
        };
        mix_contextual_ad(&mut response, matched, &context(), 20);
        assert_eq!(ad_positions(&response).len(), 1);
    }

    #[test]
    fn contextual_mix_reports_the_rendered_item_count() {
        let mut response = full_page();
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                request_id: "ad-request-3".to_string(),
                items: vec![feed_ad("campaign-3", "trail shoes", 42.0)],
                degraded: false,
            },
            &context(),
            20,
        );

        assert_eq!(response.items.len(), 20);
        assert_eq!(response.meta.as_ref().map(|meta| meta.selected), Some(20));
    }

    #[test]
    fn contextual_ad_matching_is_case_insensitive_for_equipment_keys() {
        let mut response = full_page();
        let mut context = context();
        context.scene_equipment = Some("Trail Shoes".to_string());
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                items: vec![feed_ad("campaign-4", "trail shoes", 42.0)],
                ..Default::default()
            },
            &context,
            20,
        );
        assert_eq!(ad_positions(&response).len(), 1);
    }

    #[test]
    fn displaced_organic_tail_yields_to_commerce_without_padding() {
        // A decision returning more ads than the schedule renders: excess
        // inventory must never pad the page beyond the mix contract.
        let mut response = full_page();
        mix_contextual_ad(
            &mut response,
            ad_pb::DecisionResponse {
                items: (0..5)
                    .map(|index| feed_ad(&format!("campaign-{index}"), "trail shoes", f64::from(index)))
                    .collect(),
                ..Default::default()
            },
            &context(),
            20,
        );
        assert!(response.items.len() <= 20);
        assert_eq!(ad_positions(&response).len(), 2);
        let rendered = u32::try_from(response.items.len()).unwrap_or(u32::MAX);
        assert_eq!(response.meta.as_ref().map(|meta| meta.selected), Some(rendered));
    }
}
