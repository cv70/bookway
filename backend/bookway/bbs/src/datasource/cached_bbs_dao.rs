use super::*;

pub(crate) struct CachedBbsDao {
    inner: Arc<dyn BbsDao>,
    contexts: bookway_cache::VersionedCache<pb::SocialContext>,
    visibilities: bookway_cache::VersionedCache<pb::SocialVisibility>,
    stats: bookway_cache::VersionedCache<pb::SocialStats>,
}

impl CachedBbsDao {
    pub(crate) fn new(inner: Arc<dyn BbsDao>, redis: Option<ConnectionManager>) -> Self {
        Self {
            inner,
            contexts: relationship_context_cache(redis.clone()),
            visibilities: relationship_visibility_cache(redis.clone()),
            // Counts live on the creator profile page, so they ride the same
            // version-stamped invalidation as the other relationship reads.
            stats: relationship_stats_cache(redis),
        }
    }

    /// Read-through with version-stamped payloads: a relationship mutation
    /// bumps the user's counter, so blocks and mutes stop being served the
    /// moment they commit instead of when a TTL happens to lapse.
    async fn cached_message<M>(
        cache: &bookway_cache::VersionedCache<M>,
        key: &str,
        load: impl Future<Output = Result<M, DaoError>>,
    ) -> Result<M, DaoError>
    where
        M: prost::Message + Default + Clone,
    {
        if let Some(value) = cache.load(key, key).await {
            return Ok(value);
        }

        let guard = cache.refresh_lock(key).await;
        if let Some(value) = cache.load(key, key).await {
            guard.release().await;
            return Ok(value);
        }
        if guard.peer_holds_lease() {
            guard.release().await;
            return Err(DaoError::CachePeerRefresh);
        }

        // Snapshot before reloading: a mutation that invalidates mid-reload
        // will bump past this stamp and retire whatever we store.
        let version = cache.version(key).await;
        let result = load.await;
        if let (Some(version), Ok(value)) = (version, result.as_ref()) {
            cache.store(key, version, value).await;
        }
        guard.release().await;
        result
    }
}

#[async_trait]
impl BbsDao for CachedBbsDao {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, DaoError> {
        Self::cached_message(&self.contexts, &relationship_identity(user_id), async move {
            self.inner.context(user_id).await
        })
        .await
    }

    async fn visibility_context(&self, user_id: &str) -> Result<pb::SocialVisibility, DaoError> {
        Self::cached_message(
            &self.visibilities,
            &relationship_identity(user_id),
            async move { self.inner.visibility_context(user_id).await },
        )
        .await
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, DaoError> {
        let context = self
            .inner
            .set_edge(user_id, target_user_id, edge, active)
            .await?;
        let user_key = relationship_identity(user_id);
        self.contexts.invalidate(&user_key).await;
        self.visibilities.invalidate(&user_key).await;
        // Every follow changes both ends' counts, so both stats caches retire.
        let target_key = relationship_identity(target_user_id);
        self.stats.invalidate(&user_key).await;
        self.stats.invalidate(&target_key).await;
        if edge == pb::SocialEdgeType::Block {
            // A block must also retire what the blocked party sees about them.
            self.contexts.invalidate(&target_key).await;
            self.visibilities.invalidate(&target_key).await;
        }
        Ok(context)
    }

    async fn list_followers(
        &self,
        user_id: &str,
        before: Option<KeysetCursor>,
        limit: u32,
    ) -> Result<Vec<FollowedEdge>, DaoError> {
        // Keyset pages are one cheap ordered index scan each; caching them
        // would buy little and would blur page boundaries while follows move.
        self.inner.list_followers(user_id, before, limit).await
    }

    async fn list_route_peers(
        &self,
        route_id: &str,
        viewer_id: &str,
        excluded_user_ids: &[String],
        before: Option<KeysetCursor>,
        limit: u32,
    ) -> Result<Vec<PeerEdge>, DaoError> {
        // The exclusion set is resolved fresh per call by the domain's
        // fail-closed visibility read; caching it would mask relationship
        // changes between pages.
        self.inner
            .list_route_peers(route_id, viewer_id, excluded_user_ids, before, limit)
            .await
    }

    async fn social_stats(&self, user_id: &str) -> Result<(u64, u64), DaoError> {
        let key = relationship_identity(user_id);
        let stats = Self::cached_message(&self.stats, &key, async move {
            let (followers, following) = self.inner.social_stats(user_id).await?;
            Ok(pb::SocialStats {
                followers,
                following,
            })
        })
        .await?;
        Ok((stats.followers, stats.following))
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, DaoError> {
        self.inner.list_route_participations(user_id).await
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, DaoError> {
        self.inner.route_context(user_id, route_ids).await
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, DaoError> {
        self.inner
            .set_route_participation(
                user_id,
                route_id,
                active,
                private_journey_id,
                intent_version,
            )
            .await
    }
}
