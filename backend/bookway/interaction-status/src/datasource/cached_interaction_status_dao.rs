use super::*;

pub(crate) struct CachedInteractionStatusDao {
    inner: Arc<dyn InteractionStatusDao>,
    contexts: bookway_cache::VersionedCache<pb::ReactionContext>,
}

impl CachedInteractionStatusDao {
    pub(crate) fn new(
        inner: Arc<dyn InteractionStatusDao>,
        redis: Option<ConnectionManager>,
    ) -> Self {
        Self {
            inner,
            contexts: reaction_context_cache(redis),
        }
    }
}

#[async_trait]
impl InteractionStatusDao for CachedInteractionStatusDao {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, DaoError> {
        // Payloads are keyed by the queried post set; every one of a user's
        // combinations shares their single invalidation scope, so any reaction
        // they make retires all of them at once.
        let entry = context_entry_key(user_id, post_ids);
        let scope = context_version_scope(user_id);
        let cache = &self.contexts;

        if let Some(value) = cache.load(&entry, &scope).await {
            return Ok(value);
        }

        let guard = cache.refresh_lock(&entry).await;
        if let Some(value) = cache.load(&entry, &scope).await {
            guard.release().await;
            return Ok(value);
        }
        if guard.peer_holds_lease() {
            guard.release().await;
            return Err(DaoError::CachePeerRefresh);
        }

        // Snapshot before reloading: a reaction that invalidates mid-reload
        // will bump past this stamp and retire whatever we store.
        let version = cache.version(&scope).await;
        let result = self.inner.context(user_id, post_ids).await;
        if let (Some(version), Ok(value)) = (version, result.as_ref()) {
            cache.store(&entry, version, value).await;
        }
        guard.release().await;
        result
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, DaoError> {
        let result = self
            .inner
            .set_reaction(user_id, post_id, reaction, active)
            .await?;
        self.contexts
            .invalidate(&context_version_scope(user_id))
            .await;
        Ok(result)
    }
}
