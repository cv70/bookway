use crate::api::pb;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) fn default_profile(user_id: &str) -> pb::AccountProfile {
    let now = timestamp(OffsetDateTime::now_utc());
    pb::AccountProfile {
        user_id: user_id.to_string(),
        display_name: if user_id == "demo-user" {
            "行路人"
        } else {
            "新行者"
        }
        .to_string(),
        avatar_url: String::new(),
        bio: String::new(),
        created_at: now.clone(),
        updated_at: now,
    }
}

pub(super) fn timestamp(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
