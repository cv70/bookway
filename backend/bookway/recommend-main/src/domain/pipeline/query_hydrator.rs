use std::collections::HashSet;

use bookway_bbs_link_api::pb::GrowthDomain;

use super::{FeedQuery, QueryHydrator};
use crate::api::pb;

pub(crate) struct DefaultQueryHydrator;

impl QueryHydrator for DefaultQueryHydrator {
    fn hydrate(&self, request: pb::FeedRequest) -> FeedQuery {
        let interests = request
            .interests
            .into_iter()
            .filter_map(|domain| GrowthDomain::try_from(domain).ok())
            .collect::<HashSet<_>>();
        let interests = if interests.is_empty() {
            [
                GrowthDomain::Learning,
                GrowthDomain::Movement,
                GrowthDomain::Travel,
            ]
            .into_iter()
            .collect()
        } else {
            interests
        };

        FeedQuery {
            interests,
            seen: request
                .seen
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .collect(),
            // Same bounds as FeedService's clamp: one limit contract, so a
            // client asking for 50 is never silently cut to 20 while the ad
            // slot math planned for 50.
            limit: request.limit.unwrap_or(20).clamp(1, 100) as usize,
            // Anonymous requests stay anonymous. There is no "demo-user":
            // a fabricated id once shared one exposure ledger, one frequency
            // cap and one experiment bucket across every anonymous visitor.
            user_id: non_empty(request.user_id),
            session_id: non_empty(request.session_id),
            surface: normalize_surface(&request.surface),
            cursor: request.cursor,
            geo_region: request.geo_region,
            device_os: request.device_os,
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_surface(surface: &str) -> String {
    match surface.trim() {
        "following" => "following".to_string(),
        _ => "home".to_string(),
    }
}
