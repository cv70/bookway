use std::collections::HashSet;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use super::api::{SocialContextDto, SocialEdgeTypeDto};

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("blocked users cannot follow each other")]
    BlockedRelationship,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
}

#[async_trait]
pub(crate) trait BbsRepository: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, RepositoryError>;
    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: SocialEdgeTypeDto,
        active: bool,
    ) -> Result<SocialContextDto, RepositoryError>;
}

pub(crate) struct MemoryBbsRepository {
    edges: RwLock<HashSet<(String, String, SocialEdgeTypeDto)>>,
}

impl MemoryBbsRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            edges: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "author-changfeng".to_string(),
                SocialEdgeTypeDto::Follow,
            )])),
        }
    }
}

#[async_trait]
impl BbsRepository for MemoryBbsRepository {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, RepositoryError> {
        let edges = self.edges.read().await;
        Ok(SocialContextDto {
            followed_author_ids: targets(&edges, user_id, SocialEdgeTypeDto::Follow),
            blocked_author_ids: targets(&edges, user_id, SocialEdgeTypeDto::Block),
            muted_author_ids: targets(&edges, user_id, SocialEdgeTypeDto::Mute),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: SocialEdgeTypeDto,
        active: bool,
    ) -> Result<SocialContextDto, RepositoryError> {
        let mut edges = self.edges.write().await;
        let key = (user_id.to_string(), target_user_id.to_string(), edge);
        if active && edge == SocialEdgeTypeDto::Follow {
            let blocked = [
                (
                    user_id.to_string(),
                    target_user_id.to_string(),
                    SocialEdgeTypeDto::Block,
                ),
                (
                    target_user_id.to_string(),
                    user_id.to_string(),
                    SocialEdgeTypeDto::Block,
                ),
            ]
            .iter()
            .any(|block| edges.contains(block));
            if blocked {
                return Err(RepositoryError::BlockedRelationship);
            }
        }
        if active && edge == SocialEdgeTypeDto::Block {
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                SocialEdgeTypeDto::Follow,
            ));
            edges.remove(&(
                target_user_id.to_string(),
                user_id.to_string(),
                SocialEdgeTypeDto::Follow,
            ));
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                SocialEdgeTypeDto::Mute,
            ));
        }
        if active {
            edges.insert(key);
        } else {
            edges.remove(&key);
        }
        drop(edges);
        self.context(user_id).await
    }
}

pub(crate) struct PostgresBbsRepository {
    pool: sqlx::PgPool,
}

impl PostgresBbsRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BbsRepository for PostgresBbsRepository {
    async fn context(&self, user_id: &str) -> Result<SocialContextDto, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_user_id, edge_type FROM social_edges WHERE source_user_id = $1 AND deleted_at IS NULL ORDER BY target_user_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let mut context = SocialContextDto {
            followed_author_ids: Vec::new(),
            blocked_author_ids: Vec::new(),
            muted_author_ids: Vec::new(),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        };
        for (target, edge_type) in rows {
            match edge_type.as_str() {
                "follow" => context.followed_author_ids.push(target),
                "block" => context.blocked_author_ids.push(target),
                "mute" => context.muted_author_ids.push(target),
                _ => {}
            }
        }
        Ok(context)
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: SocialEdgeTypeDto,
        active: bool,
    ) -> Result<SocialContextDto, RepositoryError> {
        let edge_type = edge_name(edge);
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        if active && edge == SocialEdgeTypeDto::Follow {
            let blocked = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM social_edges WHERE edge_type = 'block' AND deleted_at IS NULL AND ((source_user_id = $1 AND target_user_id = $2) OR (source_user_id = $2 AND target_user_id = $1)))",
            )
            .bind(user_id)
            .bind(target_user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
            if blocked {
                return Err(RepositoryError::BlockedRelationship);
            }
        }
        if active && edge == SocialEdgeTypeDto::Block {
            sqlx::query(
                "UPDATE social_edges SET deleted_at = now() WHERE deleted_at IS NULL AND ((edge_type = 'follow' AND ((source_user_id = $1 AND target_user_id = $2) OR (source_user_id = $2 AND target_user_id = $1))) OR (edge_type = 'mute' AND source_user_id = $1 AND target_user_id = $2))",
            )
            .bind(user_id)
            .bind(target_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        if active {
            sqlx::query(
                "INSERT INTO social_edges (source_user_id, target_user_id, edge_type) VALUES ($1, $2, $3) ON CONFLICT (source_user_id, target_user_id, edge_type) DO UPDATE SET deleted_at = NULL, created_at = now()",
            )
            .bind(user_id)
            .bind(target_user_id)
            .bind(edge_type)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        } else {
            sqlx::query(
                "UPDATE social_edges SET deleted_at = now() WHERE source_user_id = $1 AND target_user_id = $2 AND edge_type = $3 AND deleted_at IS NULL",
            )
            .bind(user_id)
            .bind(target_user_id)
            .bind(edge_type)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        self.context(user_id).await
    }
}

fn edge_name(edge: SocialEdgeTypeDto) -> &'static str {
    match edge {
        SocialEdgeTypeDto::Follow => "follow",
        SocialEdgeTypeDto::Block => "block",
        SocialEdgeTypeDto::Mute => "mute",
    }
}

fn targets(
    edges: &HashSet<(String, String, SocialEdgeTypeDto)>,
    user_id: &str,
    edge_type: SocialEdgeTypeDto,
) -> Vec<String> {
    edges
        .iter()
        .filter(|(source, _, edge)| source == user_id && *edge == edge_type)
        .map(|(_, target, _)| target.clone())
        .collect()
}
