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
            limit: request.limit.unwrap_or(10).clamp(1, 20) as usize,
            user_id: if request.user_id.trim().is_empty() {
                "demo-user".to_string()
            } else {
                request.user_id
            },
            session_id: if request.session_id.trim().is_empty() {
                "anonymous-session".to_string()
            } else {
                request.session_id
            },
            surface: normalize_surface(&request.surface),
            cursor: request.cursor,
        }
    }
}

fn normalize_surface(surface: &str) -> String {
    match surface.trim() {
        "following" => "following".to_string(),
        _ => "home".to_string(),
    }
}
