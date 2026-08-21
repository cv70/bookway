use crate::api::pb;

use crate::domain::{AccountError, Domain};

const MAX_DISPLAY_NAME_LENGTH: usize = 40;
const MAX_BIO_LENGTH: usize = 160;
const MAX_AVATAR_URL_LENGTH: usize = 2_048;

impl Domain {
    /// Profiles are lazily provisioned from Gateway's verified identity, like
    /// the reference user-profile service's get-or-create behaviour.
    pub(crate) async fn profile(&self, user_id: &str) -> Result<pb::AccountProfile, AccountError> {
        let user_id = validate_user_id(user_id)?;
        self.Dao
            .get_or_create(&user_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_profile(
        &self,
        request: pb::UpdateProfileRequest,
    ) -> Result<pb::AccountProfile, AccountError> {
        let user_id = validate_user_id(&request.user_id)?;
        let request = normalize_update(request)?;
        if request.display_name.is_none() && request.avatar_url.is_none() && request.bio.is_none() {
            return Err(AccountError::Validation(
                "至少提供一项要更新的资料".to_string(),
            ));
        }
        self.Dao
            .update(&user_id, request)
            .await
            .map_err(Into::into)
    }
}

fn validate_user_id(value: &str) -> Result<String, AccountError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err(AccountError::Validation("账户 ID 无效".to_string()));
    }
    Ok(value.to_string())
}

fn normalize_update(
    request: pb::UpdateProfileRequest,
) -> Result<pb::UpdateProfileRequest, AccountError> {
    let display_name = request
        .display_name
        .map(|value| value.trim().to_string())
        .map(|value| {
            if value.is_empty() || value.chars().count() > MAX_DISPLAY_NAME_LENGTH {
                Err(AccountError::Validation(
                    "昵称长度应为 1 到 40 个字符".to_string(),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()?;
    let avatar_url = request
        .avatar_url
        .map(|value| value.trim().to_string())
        .map(|value| {
            if value.len() > MAX_AVATAR_URL_LENGTH {
                Err(AccountError::Validation("头像地址过长".to_string()))
            } else if !value.is_empty()
                && !(value.starts_with("https://") || value.starts_with("http://"))
            {
                Err(AccountError::Validation(
                    "头像地址必须是 http 或 https URL".to_string(),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()?;
    let bio = request
        .bio
        .map(|value| value.trim().to_string())
        .map(|value| {
            if value.chars().count() > MAX_BIO_LENGTH {
                Err(AccountError::Validation(
                    "个人简介不能超过 160 个字符".to_string(),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()?;
    Ok(pb::UpdateProfileRequest {
        user_id: request.user_id,
        display_name,
        avatar_url,
        bio,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{conf::Config, datasource::MemoryAccountDao, domain::Domain};

    fn domain() -> Domain {
        Domain::from_dao(
            Config {
                listen_addr: "127.0.0.1:0".parse().expect("valid address"),
            },
            Arc::new(MemoryAccountDao::default()),
        )
    }

    #[tokio::test]
    async fn lazily_creates_a_profile_for_a_verified_identity() {
        let profile = domain().profile("member-1").await.expect("profile");
        assert_eq!(profile.user_id, "member-1");
        assert_eq!(profile.display_name, "新行者");
    }

    #[tokio::test]
    async fn updates_only_the_requested_profile_fields() {
        let domain = domain();
        let profile = domain
            .update_profile(pb::UpdateProfileRequest {
                user_id: "member-1".to_string(),
                display_name: Some("小满".to_string()),
                avatar_url: None,
                bio: Some("慢慢走，也会很远".to_string()),
            })
            .await
            .expect("updated profile");
        assert_eq!(profile.display_name, "小满");
        assert_eq!(profile.bio, "慢慢走，也会很远");
        assert!(profile.avatar_url.is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_profile_updates() {
        let error = domain()
            .update_profile(pb::UpdateProfileRequest {
                user_id: "member-1".to_string(),
                display_name: Some(" ".to_string()),
                avatar_url: None,
                bio: None,
            })
            .await
            .expect_err("empty name must fail");
        assert!(matches!(error, AccountError::Validation(_)));
    }
}
