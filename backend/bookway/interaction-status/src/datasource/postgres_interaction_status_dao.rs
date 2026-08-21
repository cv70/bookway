use super::*;

pub(crate) struct PostgresInteractionStatusDao {
    pool: sqlx::PgPool,
}

impl PostgresInteractionStatusDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InteractionStatusDao for PostgresInteractionStatusDao {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, DaoError> {
        if post_ids.is_empty() {
            return Ok(pb::ReactionContext {
                liked_post_ids: Vec::new(),
                bookmarked_post_ids: Vec::new(),
                hidden_post_ids: Vec::new(),
            });
        }
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_id, reaction_type FROM reactions WHERE user_id = $1 AND target_type = 'post' AND target_id = ANY($2) AND deleted_at IS NULL",
        ).bind(user_id).bind(post_ids).fetch_all(&self.pool).await.map_err(DaoError::Database)?;
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
    ) -> Result<pb::Reaction, DaoError> {
        let kind = match pb::ReactionType::try_from(reaction).ok() {
            Some(pb::ReactionType::Like) => "like",
            Some(pb::ReactionType::Bookmark) => "bookmark",
            Some(pb::ReactionType::Hide) => "hide",
            None => return Ok(pb::Reaction::default()),
        };
        if active {
            sqlx::query("INSERT INTO reactions (user_id,target_type,target_id,reaction_type,deleted_at) VALUES ($1,'post',$2,$3,NULL) ON CONFLICT (user_id,target_type,target_id,reaction_type) DO UPDATE SET deleted_at = NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(DaoError::Database)?;
        } else {
            sqlx::query("UPDATE reactions SET deleted_at = now() WHERE user_id=$1 AND target_type='post' AND target_id=$2 AND reaction_type=$3 AND deleted_at IS NULL")
                .bind(user_id).bind(post_id).bind(kind).execute(&self.pool).await.map_err(DaoError::Database)?;
        }
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM reactions WHERE target_type='post' AND target_id=$1 AND reaction_type=$2 AND deleted_at IS NULL")
            .bind(post_id).bind(kind).fetch_one(&self.pool).await.map_err(DaoError::Database)?;
        Ok(pb::Reaction {
            target_id: post_id.to_string(),
            target_type: "post".to_string(),
            reaction,
            active,
            count: count.max(0) as u64,
        })
    }
}
