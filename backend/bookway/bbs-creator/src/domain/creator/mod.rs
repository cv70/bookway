use std::collections::BTreeSet;

use crate::{
    api::pb,
    datasource::{CreatorCursor, CreatorProfileInput},
    domain::{CreatorError, Domain},
};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 50;
const MAX_USER_ID_LENGTH: usize = 160;
const MAX_SPECIALTIES: usize = 12;
const MAX_FEATURED_CONTENT: usize = 6;

impl Domain {
    pub(crate) async fn get_profile(
        &self,
        request: pb::CreatorProfileRequest,
    ) -> Result<pb::CreatorProfile, CreatorError> {
        validate_user_id(&request.user_id)?;
        Ok(self.repository.get(request.user_id.trim()).await?)
    }

    pub(crate) async fn upsert_profile(
        &self,
        request: pb::UpsertCreatorProfileRequest,
    ) -> Result<pb::CreatorProfile, CreatorError> {
        validate_user_id(&request.user_id)?;
        let handle = normalize_handle(&request.handle)?;
        let headline = normalized_text("创作者简介标题", &request.headline, 80, false)?;
        let introduction = normalized_text("创作者介绍", &request.introduction, 2_000, true)?;
        let cover_url = normalized_url("封面地址", &request.cover_url)?;
        let specialties = normalized_strings("擅长领域", request.specialties, MAX_SPECIALTIES, 32)?;
        let featured_content_ids = normalized_strings(
            "精选内容 ID",
            request.featured_content_ids,
            MAX_FEATURED_CONTENT,
            160,
        )?;
        let state = pb::CreatorState::try_from(request.state)
            .map_err(|_| CreatorError::Validation("创作者状态无效".to_string()))?;
        Ok(self
            .repository
            .upsert(CreatorProfileInput {
                user_id: request.user_id.trim().to_string(),
                handle,
                headline,
                introduction,
                cover_url,
                specialties,
                featured_content_ids,
                state: state as i32,
            })
            .await?)
    }

    pub(crate) async fn list_profiles(
        &self,
        request: pb::ListCreatorProfilesRequest,
    ) -> Result<pb::CreatorProfilePage, CreatorError> {
        if request.user_ids.len() > MAX_PAGE_SIZE {
            return Err(CreatorError::Validation(
                "单次最多查询 50 位创作者".to_string(),
            ));
        }
        let user_ids = normalized_strings(
            "用户 ID",
            request.user_ids,
            MAX_PAGE_SIZE,
            MAX_USER_ID_LENGTH,
        )?;
        let excluded_user_ids = normalized_strings(
            "排除用户 ID",
            request.excluded_user_ids,
            500,
            MAX_USER_ID_LENGTH,
        )?;
        let query = request
            .query
            .as_deref()
            .map(|value| normalized_text("搜索词", value, 100, false))
            .transpose()?;
        let specialty = request
            .specialty
            .as_deref()
            .map(|value| normalized_text("擅长领域", value, 32, false))
            .transpose()?;
        let cursor = match request.cursor.as_deref() {
            Some(value) => Some(
                CreatorCursor::decode(value)
                    .ok_or_else(|| CreatorError::Validation("创作者游标无效".to_string()))?,
            ),
            None => None,
        };
        let limit = request
            .limit
            .unwrap_or(DEFAULT_PAGE_SIZE as u32)
            .clamp(1, MAX_PAGE_SIZE as u32) as usize;
        let mut items = self
            .repository
            .list(
                &user_ids,
                &excluded_user_ids,
                query.as_deref(),
                specialty.as_deref(),
                cursor.as_ref(),
                limit + 1,
            )
            .await?;
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().and_then(CreatorCursor::from_profile))
            .flatten()
            .map(|cursor| cursor.encode());
        Ok(pb::CreatorProfilePage { items, next_cursor })
    }
}

fn validate_user_id(user_id: &str) -> Result<(), CreatorError> {
    let value = user_id.trim();
    if value.is_empty() || value.chars().count() > MAX_USER_ID_LENGTH {
        return Err(CreatorError::Validation("用户 ID 无效".to_string()));
    }
    Ok(())
}

fn normalize_handle(value: &str) -> Result<String, CreatorError> {
    let handle = value.trim().to_ascii_lowercase();
    let valid = (3..=32).contains(&handle.len())
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if !valid {
        return Err(CreatorError::Validation(
            "创作者昵称须为 3-32 位小写字母、数字或下划线".to_string(),
        ));
    }
    Ok(handle)
}

fn normalized_text(
    label: &str,
    value: &str,
    max_chars: usize,
    allow_empty: bool,
) -> Result<String, CreatorError> {
    let value = value.trim();
    if (!allow_empty && value.is_empty()) || value.chars().count() > max_chars {
        return Err(CreatorError::Validation(format!("{label} 无效")));
    }
    Ok(value.to_string())
}

fn normalized_url(label: &str, value: &str) -> Result<String, CreatorError> {
    let value = value.trim();
    if value.chars().count() > 2_048
        || (!value.is_empty() && !value.starts_with("https://") && !value.starts_with("http://"))
    {
        return Err(CreatorError::Validation(format!(
            "{label} 必须是 HTTP(S) 地址"
        )));
    }
    Ok(value.to_string())
}

fn normalized_strings(
    label: &str,
    values: Vec<String>,
    max_items: usize,
    max_chars: usize,
) -> Result<Vec<String>, CreatorError> {
    if values.len() > max_items {
        return Err(CreatorError::Validation(format!("{label} 数量超出限制")));
    }
    let values = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    if values.iter().any(|value| value.chars().count() > max_chars) {
        return Err(CreatorError::Validation(format!("{label} 包含无效值")));
    }
    Ok(values.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryCreatorRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid HTTP address"),
                grpc_addr: "127.0.0.1:0".parse().expect("valid gRPC address"),
            },
            Arc::new(MemoryCreatorRepository::default()),
        )
    }

    fn profile(user_id: &str, handle: &str) -> pb::UpsertCreatorProfileRequest {
        pb::UpsertCreatorProfileRequest {
            user_id: user_id.to_string(),
            handle: handle.to_string(),
            headline: "把阅读变成行动".to_string(),
            introduction: "从真实实践中整理可复用路线。".to_string(),
            cover_url: "https://cdn.example/cover.jpg".to_string(),
            specialties: vec!["阅读".to_string(), "知识管理".to_string()],
            featured_content_ids: vec!["post-a".to_string()],
            state: pb::CreatorState::Active as i32,
        }
    }

    #[tokio::test]
    async fn handle_is_case_insensitive_and_unique() {
        let domain = domain();
        let first = domain
            .upsert_profile(profile("creator-a", "Action_Reader"))
            .await
            .expect("create profile");
        assert_eq!(first.handle, "action_reader");

        let error = domain
            .upsert_profile(profile("creator-b", "ACTION_READER"))
            .await
            .expect_err("duplicate handle");
        assert!(matches!(error, CreatorError::Repository(_)));
    }

    #[tokio::test]
    async fn listing_has_a_stable_cursor() {
        let domain = domain();
        domain
            .upsert_profile(profile("creator-a", "reader_a"))
            .await
            .expect("first profile");
        domain
            .upsert_profile(profile("creator-b", "reader_b"))
            .await
            .expect("second profile");

        let first = domain
            .list_profiles(pb::ListCreatorProfilesRequest {
                user_ids: Vec::new(),
                query: None,
                specialty: None,
                cursor: None,
                limit: Some(1),
                excluded_user_ids: Vec::new(),
            })
            .await
            .expect("first page");
        let second = domain
            .list_profiles(pb::ListCreatorProfilesRequest {
                user_ids: Vec::new(),
                query: None,
                specialty: None,
                cursor: first.next_cursor,
                limit: Some(1),
                excluded_user_ids: Vec::new(),
            })
            .await
            .expect("second page");
        assert_eq!(first.items.len(), 1);
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].user_id, second.items[0].user_id);
    }

    #[tokio::test]
    async fn discovery_hides_paused_and_excluded_creators_without_hiding_explicit_lookup() {
        let domain = domain();
        let mut paused = profile("creator-paused", "reader_paused");
        paused.state = pb::CreatorState::Paused as i32;
        domain.upsert_profile(paused).await.expect("paused profile");
        domain
            .upsert_profile(profile("creator-active", "reader_active"))
            .await
            .expect("active profile");

        let discovery = domain
            .list_profiles(pb::ListCreatorProfilesRequest {
                user_ids: Vec::new(),
                query: None,
                specialty: None,
                cursor: None,
                limit: None,
                excluded_user_ids: vec!["creator-active".to_string()],
            })
            .await
            .expect("filtered discovery");
        assert!(discovery.items.is_empty());

        let explicit = domain
            .list_profiles(pb::ListCreatorProfilesRequest {
                user_ids: vec!["creator-paused".to_string()],
                query: None,
                specialty: None,
                cursor: None,
                limit: None,
                excluded_user_ids: Vec::new(),
            })
            .await
            .expect("explicit paused profile");
        assert_eq!(explicit.items[0].user_id, "creator-paused");
    }
}
