use super::*;

#[derive(Default)]
pub(crate) struct MemoryMediaDao {
    assets: RwLock<HashMap<String, (String, pb::MediaResource)>>,
}

#[async_trait]
impl MediaDao for MemoryMediaDao {
    async fn create(&self, media: NewMedia) -> Result<pb::MediaResource, DaoError> {
        let response = to_response(&media, "pending");
        self.assets
            .write()
            .await
            .insert(media.id, (media.owner_id, response.clone()));
        Ok(response)
    }
    async fn pending(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id && media.status == "pending")
            .map(|(_, media)| media.clone())
            .ok_or(DaoError::NotFound)
    }
    async fn owned(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, _)| owner == owner_id)
            .map(|(_, media)| media.clone())
            .ok_or(DaoError::NotFound)
    }
    async fn get(&self, id: &str, owner_id: &str) -> Result<pb::MediaResource, DaoError> {
        self.assets
            .read()
            .await
            .get(id)
            .filter(|(owner, media)| owner == owner_id || media.status == "ready")
            .map(|(_, media)| media.clone())
            .ok_or(DaoError::NotFound)
    }
    async fn mark_processing(&self, id: &str) -> Result<pb::MediaResource, DaoError> {
        let mut assets = self.assets.write().await;
        let (_, media) = assets.get_mut(id).ok_or(DaoError::NotFound)?;
        // Memory storage has no independently running processor. It is the
        // deterministic local executor for the same already-validated asset.
        media.status = "ready".to_string();
        Ok(media.clone())
    }

    async fn owned_ready_batch(
        &self,
        owner_id: &str,
        ids: &[String],
    ) -> Result<Vec<pb::MediaResource>, DaoError> {
        let assets = self.assets.read().await;
        ids.iter()
            .map(|id| {
                assets
                    .get(id)
                    .filter(|(owner, media)| owner == owner_id && media.status == "ready")
                    .map(|(_, media)| media.clone())
                    .ok_or(DaoError::NotFound)
            })
            .collect()
    }
}
