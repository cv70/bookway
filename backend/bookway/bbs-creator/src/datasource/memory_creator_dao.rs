use super::*;

#[derive(Default)]
pub(crate) struct MemoryCreatorDao {
    profiles: RwLock<HashMap<String, pb::CreatorProfile>>,
}

#[async_trait]
impl CreatorDao for MemoryCreatorDao {
    async fn get(&self, user_id: &str) -> Result<pb::CreatorProfile, DaoError> {
        self.profiles
            .read()
            .await
            .get(user_id)
            .cloned()
            .ok_or_else(|| DaoError::NotFound(user_id.to_string()))
    }

    async fn upsert(&self, input: CreatorProfileInput) -> Result<pb::CreatorProfile, DaoError> {
        let mut profiles = self.profiles.write().await;
        if profiles
            .values()
            .any(|profile| profile.user_id != input.user_id && profile.handle == input.handle)
        {
            return Err(DaoError::HandleTaken(input.handle));
        }
        let now = now_timestamp();
        let created_at = profiles
            .get(&input.user_id)
            .map(|profile| profile.created_at.clone())
            .unwrap_or_else(|| now.clone());
        let profile = pb::CreatorProfile {
            user_id: input.user_id.clone(),
            handle: input.handle,
            headline: input.headline,
            introduction: input.introduction,
            cover_url: input.cover_url,
            specialties: input.specialties,
            featured_content_ids: input.featured_content_ids,
            state: input.state,
            created_at,
            updated_at: now,
        };
        profiles.insert(input.user_id, profile.clone());
        Ok(profile)
    }

    async fn list(
        &self,
        user_ids: &[String],
        excluded_user_ids: &[String],
        query: Option<&str>,
        specialty: Option<&str>,
        cursor: Option<&CreatorCursor>,
        limit: usize,
    ) -> Result<Vec<pb::CreatorProfile>, DaoError> {
        let query = query.map(str::to_lowercase);
        let specialty = specialty.map(str::to_lowercase);
        let mut profiles = self
            .profiles
            .read()
            .await
            .values()
            .filter(|profile| user_ids.is_empty() || user_ids.contains(&profile.user_id))
            .filter(|profile| !excluded_user_ids.contains(&profile.user_id))
            .filter(|profile| {
                !user_ids.is_empty() || profile.state == pb::CreatorState::Active as i32
            })
            .filter(|profile| {
                query.as_ref().is_none_or(|query| {
                    [
                        profile.handle.as_str(),
                        profile.headline.as_str(),
                        profile.introduction.as_str(),
                    ]
                    .into_iter()
                    .any(|field| field.to_lowercase().contains(query))
                })
            })
            .filter(|profile| {
                specialty.as_ref().is_none_or(|specialty| {
                    profile
                        .specialties
                        .iter()
                        .any(|value| value.to_lowercase() == *specialty)
                })
            })
            .filter(|profile| {
                cursor.is_none_or(|cursor| {
                    OffsetDateTime::parse(&profile.updated_at, &Rfc3339).is_ok_and(|updated_at| {
                        (updated_at, profile.user_id.as_str())
                            < (cursor.updated_at, cursor.user_id.as_str())
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| {
            let left_at = OffsetDateTime::parse(&left.updated_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            let right_at = OffsetDateTime::parse(&right.updated_at, &Rfc3339)
                .unwrap_or(OffsetDateTime::UNIX_EPOCH);
            right_at
                .cmp(&left_at)
                .then_with(|| right.user_id.cmp(&left.user_id))
        });
        profiles.truncate(limit);
        Ok(profiles)
    }
}
