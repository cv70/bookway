use std::collections::HashMap;

use async_trait::async_trait;
use time::OffsetDateTime;
use tokio::sync::RwLock;

use crate::api::pb;

use super::{
    AccountDao, DaoError,
    account_profile::{default_profile, timestamp},
};

#[derive(Default)]
pub(crate) struct MemoryAccountDao {
    profiles: RwLock<HashMap<String, pb::AccountProfile>>,
}

#[async_trait]
impl AccountDao for MemoryAccountDao {
    async fn get_or_create(&self, user_id: &str) -> Result<pb::AccountProfile, DaoError> {
        let mut profiles = self.profiles.write().await;
        Ok(profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_profile(user_id))
            .clone())
    }

    async fn update(
        &self,
        user_id: &str,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, DaoError> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .entry(user_id.to_string())
            .or_insert_with(|| default_profile(user_id));
        if let Some(display_name) = request.display_name {
            profile.display_name = display_name;
        }
        if let Some(avatar_url) = request.avatar_url {
            profile.avatar_url = avatar_url;
        }
        if let Some(bio) = request.bio {
            profile.bio = bio;
        }
        profile.updated_at = timestamp(OffsetDateTime::now_utc());
        Ok(profile.clone())
    }
}
