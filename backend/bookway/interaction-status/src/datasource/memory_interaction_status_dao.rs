use super::*;

pub(crate) struct MemoryInteractionStatusDao {
    reactions: RwLock<HashSet<(String, String, i32)>>,
}

impl MemoryInteractionStatusDao {
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
impl InteractionStatusDao for MemoryInteractionStatusDao {
    async fn context(
        &self,
        user_id: &str,
        post_ids: &[String],
    ) -> Result<pb::ReactionContext, DaoError> {
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
    ) -> Result<pb::Reaction, DaoError> {
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
