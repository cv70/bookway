use super::*;

pub(crate) struct MemoryBbsDao {
    edges: RwLock<HashSet<(String, String, pb::SocialEdgeType)>>,
    route_participations: RwLock<HashMap<(String, String), pb::RouteParticipation>>,
    route_intent_versions: RwLock<HashMap<(String, String), u64>>,
}

impl MemoryBbsDao {
    pub(crate) fn seeded() -> Self {
        Self {
            edges: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "author-changfeng".to_string(),
                pb::SocialEdgeType::Follow,
            )])),
            route_participations: RwLock::new(HashMap::new()),
            route_intent_versions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl BbsDao for MemoryBbsDao {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, DaoError> {
        let edges = self.edges.read().await;
        Ok(pb::SocialContext {
            followed_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Follow),
            blocked_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Block),
            muted_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Mute),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        })
    }

    async fn visibility_context(&self, user_id: &str) -> Result<pb::SocialVisibility, DaoError> {
        let edges = self.edges.read().await;
        let mut excluded_author_ids = edges
            .iter()
            .filter_map(|(source, target, edge)| match edge {
                pb::SocialEdgeType::Block | pb::SocialEdgeType::Mute if source == user_id => {
                    Some(target.clone())
                }
                pb::SocialEdgeType::Block if target == user_id => Some(source.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        excluded_author_ids.sort();
        excluded_author_ids.dedup();
        Ok(pb::SocialVisibility {
            excluded_author_ids,
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, DaoError> {
        let mut edges = self.edges.write().await;
        let key = (user_id.to_string(), target_user_id.to_string(), edge);
        if active && edge == pb::SocialEdgeType::Follow {
            let blocked = [
                (
                    user_id.to_string(),
                    target_user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
                (
                    target_user_id.to_string(),
                    user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
            ]
            .iter()
            .any(|block| edges.contains(block));
            if blocked {
                return Err(DaoError::BlockedRelationship);
            }
        }
        if active && edge == pb::SocialEdgeType::Block {
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                target_user_id.to_string(),
                user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Mute,
            ));
        }
        if active {
            edges.insert(key);
        } else {
            edges.remove(&key);
        }
        drop(edges);
        self.context(user_id).await
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, DaoError> {
        let participations = self.route_participations.read().await;
        let mut items = participations
            .iter()
            .filter(|((_, participant_id), _)| participant_id == user_id)
            .map(|(_, participation)| participation.clone())
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.joined_at.cmp(&left.joined_at));
        Ok(items)
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, DaoError> {
        let requested = route_ids.iter().collect::<HashSet<_>>();
        let participations = self.route_participations.read().await;
        let mut context = pb::RouteParticipationContext::default();
        for (route_id, participant_id) in participations.keys() {
            if !requested.contains(route_id) {
                continue;
            }
            *context
                .participant_counts
                .entry(route_id.clone())
                .or_default() += 1;
            if participant_id == user_id {
                context.joined_route_ids.push(route_id.clone());
            }
        }
        context.joined_route_ids.sort();
        Ok(context)
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, DaoError> {
        let key = (route_id.to_string(), user_id.to_string());
        let mut versions = self.route_intent_versions.write().await;
        let mut participations = self.route_participations.write().await;
        let current_version = versions.get(&key).copied().unwrap_or_default();
        let accepted = command_is_accepted(current_version, intent_version);
        if accepted && let Some(version) = intent_version {
            versions.insert(key.clone(), version);
        }
        if accepted && active {
            let joined_at = participations
                .get(&key)
                .map(|item| item.joined_at.clone())
                .unwrap_or(format_timestamp(time::OffsetDateTime::now_utc())?);
            participations.insert(
                key.clone(),
                pb::RouteParticipation {
                    route_id: route_id.to_string(),
                    private_journey_id: private_journey_id.clone(),
                    joined_at: joined_at.clone(),
                },
            );
        } else if accepted {
            participations.remove(&key);
        }
        let participant_count = participations
            .keys()
            .filter(|(current_route_id, _)| current_route_id == route_id)
            .count() as u64;
        let participation = participations.get(&key);
        Ok(pb::RouteParticipationState {
            route_id: route_id.to_string(),
            joined: participation.is_some(),
            private_journey_id: participation.and_then(|item| item.private_journey_id.clone()),
            joined_at: participation.map(|item| item.joined_at.clone()),
            participant_count,
        })
    }
}
