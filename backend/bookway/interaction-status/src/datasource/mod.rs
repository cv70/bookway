use std::collections::HashSet;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::pb;

#[async_trait]
pub(crate) trait InteractionStatusRepository: Send + Sync {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError>;
    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError>;
}

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

pub(crate) struct MemoryInteractionStatusRepository {
    reactions: RwLock<HashSet<(String, String, i32)>>,
}

impl MemoryInteractionStatusRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            reactions: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "post-reading".to_string(),
                pb::ReactionType::Like as i32,
            )])),
        }
    }
}

#[async_trait]
impl InteractionStatusRepository for MemoryInteractionStatusRepository {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        let reactions = self.reactions.read().await;
        Ok(pb::ReactionContext {
            liked_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Like as i32,
            ),
            bookmarked_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Bookmark as i32,
            ),
            hidden_post_ids: matching_post_ids(
                &reactions,
                user_id,
                post_ids,
                pb::ReactionType::Hide as i32,
            ),
        })
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError> {
        let mut reactions = self.reactions.write().await;
        let key = (user_id.to_string(), post_id.to_string(), reaction);
        if active {
            reactions.insert(key);
        } else {
            reactions.remove(&key);
        }
        let count = reactions
            .iter()
            .filter(|(_, target, kind)| target == post_id && *kind == reaction)
            .count() as u64;
        Ok(pb::Reaction {
            target_id: post_id.to_string(),
            target_type: "post".to_string(),
            reaction,
            active,
            count,
        })
    }
}

pub(crate) struct PostgresInteractionStatusRepository {
    pool: sqlx::PgPool,
}

impl PostgresInteractionStatusRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InteractionStatusRepository for PostgresInteractionStatusRepository {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, RepositoryError> {
        if post_ids.is_empty() {
            return Ok(pb::ReactionContext {
                liked_post_ids: Vec::new(),
                bookmarked_post_ids: Vec::new(),
                hidden_post_ids: Vec::new(),
            });
        }
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_id, reaction_type FROM reactions WHERE user_id = $1 AND target_type = 'post' AND target_id = ANY($2) AND deleted_at IS NULL",
        ).bind(user_id).bind(post_ids).fetch_all(&self.pool).await.map_err(RepositoryError::Database)?;
        let mut result = pb::ReactionContext {
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
            hidden_post_ids: Vec::new(),
        };
        for (target, kind) in rows {
            match kind.as_str() {
                "like" => result.liked_post_ids.push(target),
                "bookmark" => result.bookmarked_post_ids.push(target),
                "hide" => result.hidden_post_ids.push(target),
                _ => {}
            }
        }
        Ok(result)
    }

    async fn set_reaction(
        &self,
        user_id: &str,
        post_id: &str,
        reaction: i32,
        active: bool,
    ) -> Result<pb::Reaction, RepositoryError> {
        let kind = match pb::ReactionType::try_from(reaction).ok() {
            Some(pb::ReactionType::Like) => "like",
            Some(pb::ReactionType::Bookmark) => "bookmark",
            Some(pb::ReactionType::Hide) => "hide",
            None => return Ok(pb::Reaction::default()),
        };
        if active {
            sqlx::query("INSERT INTO reactions (user_id,target_type,target_id,reaction_type,deleted_at) VALUES ($1,'post',$2,$3,NULL) ON CONFLICT (user_id,target_type,target_id,reaction_type) DO UPDATE SET deleted_at = NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        } else {
            sqlx::query("UPDATE reactions SET deleted_at = now() WHERE user_id=$1 AND target_type='post' AND target_id=$2 AND reaction_type=$3 AND deleted_at IS NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(RepositoryError::Database)?;
        }
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM reactions WHERE target_type='post' AND target_id=$1 AND reaction_type=$2 AND deleted_at IS NULL")
            .bind(post_id).bind(kind).fetch_one(&self.pool).await.map_err(RepositoryError::Database)?;
        Ok(pb::Reaction {
            target_id: post_id.to_string(),
            target_type: "post".to_string(),
            reaction,
            active,
            count: count.max(0) as u64,
        })
    }
}

fn matching_post_ids(
    reactions: &HashSet<(String, String, i32)>,
    user_id: &str,
    post_ids: &[String],
    reaction: i32,
) -> Vec<String> {
    post_ids
        .iter()
        .filter(|post_id| reactions.contains(&(user_id.to_string(), (*post_id).clone(), reaction)))
        .cloned()
        .collect()
}
