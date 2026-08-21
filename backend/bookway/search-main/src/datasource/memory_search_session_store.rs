use super::*;

#[derive(Default)]
pub(crate) struct MemorySearchSessionStore {
    sessions: RwLock<HashMap<String, (SearchPipelineSession, Instant)>>,
}

#[async_trait]
impl SearchSessionStore for MemorySearchSessionStore {
    async fn create(&self, session: SearchPipelineSession) -> Result<String, SearchSessionError> {
        let id = Uuid::now_v7().to_string();
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        sessions.insert(id.clone(), (session, now + SEARCH_MAIN_SESSION_TTL));
        Ok(id)
    }

    async fn load(&self, id: &str) -> Result<Option<SearchPipelineSession>, SearchSessionError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        Ok(sessions.get(id).map(|(session, _)| session.clone()))
    }

    async fn save(
        &self,
        id: &str,
        session: SearchPipelineSession,
    ) -> Result<bool, SearchSessionError> {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires_at)| *expires_at > now);
        let Some((stored, expires_at)) = sessions.get_mut(id) else {
            return Ok(false);
        };
        *stored = session;
        *expires_at = now + SEARCH_MAIN_SESSION_TTL;
        Ok(true)
    }

    async fn delete(&self, id: &str) -> Result<(), SearchSessionError> {
        self.sessions.write().await.remove(id);
        Ok(())
    }
}
