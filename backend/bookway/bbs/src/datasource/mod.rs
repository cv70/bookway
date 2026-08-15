use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::api::pb;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("blocked users cannot follow each other")]
    BlockedRelationship,
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("timestamp formatting failed: {0}")]
    Timestamp(#[from] time::error::Format),
}

#[async_trait]
pub(crate) trait BbsRepository: Send + Sync {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError>;
    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError>;
    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError>;
    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError>;
    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError>;
    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError>;
}

pub(crate) struct MemoryBbsRepository {
    edges: RwLock<HashSet<(String, String, pb::SocialEdgeType)>>,
    route_participations: RwLock<HashMap<(String, String), pb::RouteParticipation>>,
    route_intent_versions: RwLock<HashMap<(String, String), u64>>,
}

impl MemoryBbsRepository {
    pub(crate) fn seeded() -> Self {
        Self {
            edges: RwLock::new(HashSet::from([(
                "demo-user".to_string(),
                "author-changfeng".to_string(),
                pb::SocialEdgeType::Follow,
            )])),
            route_participations: RwLock::new(HashMap::new()),
            route_intent_versions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl BbsRepository for MemoryBbsRepository {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError> {
        let edges = self.edges.read().await;
        Ok(pb::SocialContext {
            followed_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Follow),
            blocked_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Block),
            muted_author_ids: targets(&edges, user_id, pb::SocialEdgeType::Mute),
            liked_post_ids: Vec::new(),
            bookmarked_post_ids: Vec::new(),
        })
    }

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError> {
        let edges = self.edges.read().await;
        let mut excluded_author_ids = edges
            .iter()
            .filter_map(|(source, target, edge)| match edge {
                pb::SocialEdgeType::Block | pb::SocialEdgeType::Mute if source == user_id => {
                    Some(target.clone())
                }
                pb::SocialEdgeType::Block if target == user_id => Some(source.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        excluded_author_ids.sort();
        excluded_author_ids.dedup();
        Ok(pb::SocialVisibility {
            excluded_author_ids,
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError> {
        let mut edges = self.edges.write().await;
        let key = (user_id.to_string(), target_user_id.to_string(), edge);
        if active && edge == pb::SocialEdgeType::Follow {
            let blocked = [
                (
                    user_id.to_string(),
                    target_user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
                (
                    target_user_id.to_string(),
                    user_id.to_string(),
                    pb::SocialEdgeType::Block,
                ),
            ]
            .iter()
            .any(|block| edges.contains(block));
            if blocked {
                return Err(RepositoryError::BlockedRelationship);
            }
        }
        if active && edge == pb::SocialEdgeType::Block {
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                target_user_id.to_string(),
                user_id.to_string(),
                pb::SocialEdgeType::Follow,
            ));
            edges.remove(&(
                user_id.to_string(),
                target_user_id.to_string(),
                pb::SocialEdgeType::Mute,
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

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError> {
        let participations = self.route_participations.read().await;
        let mut items = participations
            .iter()
            .filter(|((_, participant_id), _)| participant_id == user_id)
            .map(|(_, participation)| participation.clone())
            .collect::<Vec<_>>();
        items.sort_by(|left, right| right.joined_at.cmp(&left.joined_at));
        Ok(items)
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError> {
        let requested = route_ids.iter().collect::<HashSet<_>>();
        let participations = self.route_participations.read().await;
        let mut context = pb::RouteParticipationContext::default();
        for (route_id, participant_id) in participations.keys() {
            if !requested.contains(route_id) {
                continue;
            }
            *context
                .participant_counts
                .entry(route_id.clone())
                .or_default() += 1;
            if participant_id == user_id {
                context.joined_route_ids.push(route_id.clone());
            }
        }
        context.joined_route_ids.sort();
        Ok(context)
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError> {
        let key = (route_id.to_string(), user_id.to_string());
        let mut versions = self.route_intent_versions.write().await;
        let mut participations = self.route_participations.write().await;
        let current_version = versions.get(&key).copied().unwrap_or_default();
        let accepted = command_is_accepted(current_version, intent_version);
        if accepted && let Some(version) = intent_version {
            versions.insert(key.clone(), version);
        }
        if accepted && active {
            let joined_at = participations
                .get(&key)
                .map(|item| item.joined_at.clone())
                .unwrap_or(format_timestamp(time::OffsetDateTime::now_utc())?);
            participations.insert(
                key.clone(),
                pb::RouteParticipation {
                    route_id: route_id.to_string(),
                    private_journey_id: private_journey_id.clone(),
                    joined_at: joined_at.clone(),
                },
            );
        } else if accepted {
            participations.remove(&key);
        }
        let participant_count = participations
            .keys()
            .filter(|(current_route_id, _)| current_route_id == route_id)
            .count() as u64;
        let participation = participations.get(&key);
        Ok(pb::RouteParticipationState {
            route_id: route_id.to_string(),
            joined: participation.is_some(),
            private_journey_id: participation.and_then(|item| item.private_journey_id.clone()),
            joined_at: participation.map(|item| item.joined_at.clone()),
            participant_count,
        })
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
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_user_id, edge_type FROM social_edges WHERE source_user_id = $1 AND deleted_at IS NULL ORDER BY target_user_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let mut context = pb::SocialContext {
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

    async fn visibility_context(
        &self,
        user_id: &str,
    ) -> Result<pb::SocialVisibility, RepositoryError> {
        let excluded_author_ids = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT CASE WHEN source_user_id = $1 THEN target_user_id ELSE source_user_id END FROM social_edges WHERE deleted_at IS NULL AND ((source_user_id = $1 AND edge_type IN ('block', 'mute')) OR (target_user_id = $1 AND edge_type = 'block')) ORDER BY 1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(pb::SocialVisibility {
            excluded_author_ids,
        })
    }

    async fn set_edge(
        &self,
        user_id: &str,
        target_user_id: &str,
        edge: pb::SocialEdgeType,
        active: bool,
    ) -> Result<pb::SocialContext, RepositoryError> {
        let edge_type = edge_name(edge);
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        let (first_user_id, second_user_id) = ordered_social_pair(user_id, target_user_id);
        // A block removes follows in both directions. Serialize every mutation
        // for this user pair so a concurrent follow cannot commit after that cleanup.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        if active && edge == pb::SocialEdgeType::Follow {
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
        if active && edge == pb::SocialEdgeType::Block {
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

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, RepositoryError> {
        let rows = sqlx::query_as::<_, (String, Option<String>, time::OffsetDateTime)>(
            "SELECT route_id, private_journey_id, joined_at FROM route_participations WHERE user_id = $1 AND left_at IS NULL ORDER BY joined_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|(route_id, private_journey_id, joined_at)| {
                Ok(pb::RouteParticipation {
                    route_id,
                    private_journey_id,
                    joined_at: format_timestamp(joined_at)?,
                })
            })
            .collect()
    }

    async fn route_context(
        &self,
        user_id: &str,
        route_ids: &[String],
    ) -> Result<pb::RouteParticipationContext, RepositoryError> {
        if route_ids.is_empty() {
            return Ok(pb::RouteParticipationContext::default());
        }
        let counts = sqlx::query_as::<_, (String, i64)>(
            "SELECT route_id, SUM(active_count)::BIGINT FROM route_participation_count_shards WHERE route_id = ANY($1) GROUP BY route_id",
        )
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        let joined_route_ids = sqlx::query_scalar::<_, String>(
            "SELECT route_id FROM route_participations WHERE user_id = $1 AND route_id = ANY($2) AND left_at IS NULL ORDER BY route_id",
        )
        .bind(user_id)
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(pb::RouteParticipationContext {
            joined_route_ids,
            participant_counts: counts
                .into_iter()
                .map(|(route_id, count)| (route_id, count.max(0) as u64))
                .collect(),
        })
    }

    async fn set_route_participation(
        &self,
        user_id: &str,
        route_id: &str,
        active: bool,
        private_journey_id: Option<String>,
        intent_version: Option<u64>,
    ) -> Result<pb::RouteParticipationState, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        // Only commands for the same user and route need ordering. Hot routes can use all shards.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(user_id)
            .bind(route_id)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;

        let intent_version =
            intent_version.map(|version| i64::try_from(version).unwrap_or(i64::MAX));

        if active {
            sqlx::query(
                "INSERT INTO route_participations (route_id, user_id, private_journey_id, left_at, last_intent_version) VALUES ($1, $2, $3, NULL, COALESCE($4, 0)) ON CONFLICT (route_id, user_id) DO UPDATE SET private_journey_id = EXCLUDED.private_journey_id, joined_at = CASE WHEN route_participations.left_at IS NULL THEN route_participations.joined_at ELSE now() END, left_at = NULL, last_intent_version = COALESCE($4, route_participations.last_intent_version) WHERE ($4 IS NOT NULL AND $4 >= route_participations.last_intent_version) OR ($4 IS NULL AND route_participations.last_intent_version = 0)",
            )
            .bind(route_id)
            .bind(user_id)
            .bind(private_journey_id)
            .bind(intent_version)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        } else {
            sqlx::query(
                "INSERT INTO route_participations (route_id, user_id, private_journey_id, left_at, last_intent_version) VALUES ($1, $2, NULL, now(), COALESCE($3, 0)) ON CONFLICT (route_id, user_id) DO UPDATE SET private_journey_id = NULL, left_at = CASE WHEN route_participations.left_at IS NULL THEN now() ELSE route_participations.left_at END, last_intent_version = COALESCE($3, route_participations.last_intent_version) WHERE ($3 IS NOT NULL AND $3 >= route_participations.last_intent_version) OR ($3 IS NULL AND route_participations.last_intent_version = 0)",
            )
            .bind(route_id)
            .bind(user_id)
            .bind(intent_version)
            .execute(&mut *transaction)
            .await
            .map_err(RepositoryError::Database)?;
        }
        let (private_journey_id, joined_at, left_at) = sqlx::query_as::<
            _,
            (
                Option<String>,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
            ),
        >(
            "SELECT private_journey_id, joined_at, left_at FROM route_participations WHERE route_id = $1 AND user_id = $2",
        )
        .bind(route_id)
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        let joined = left_at.is_none();
        let participant_count = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(active_count), 0)::BIGINT FROM route_participation_count_shards WHERE route_id = $1",
        )
        .bind(route_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(pb::RouteParticipationState {
            route_id: route_id.to_string(),
            joined,
            private_journey_id: joined.then_some(private_journey_id).flatten(),
            joined_at: if joined {
                Some(format_timestamp(joined_at)?)
            } else {
                None
            },
            participant_count: participant_count.max(0) as u64,
        })
    }
}

fn command_is_accepted(current_version: u64, incoming_version: Option<u64>) -> bool {
    incoming_version
        .map(|version| version >= current_version)
        .unwrap_or(current_version == 0)
}

fn format_timestamp(value: time::OffsetDateTime) -> Result<String, time::error::Format> {
    value.format(&time::format_description::well_known::Rfc3339)
}

fn edge_name(edge: pb::SocialEdgeType) -> &'static str {
    match edge {
        pb::SocialEdgeType::Follow => "follow",
        pb::SocialEdgeType::Block => "block",
        pb::SocialEdgeType::Mute => "mute",
    }
}

fn ordered_social_pair<'a>(first: &'a str, second: &'a str) -> (&'a str, &'a str) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn targets(
    edges: &HashSet<(String, String, pb::SocialEdgeType)>,
    user_id: &str,
    edge_type: pb::SocialEdgeType,
) -> Vec<String> {
    edges
        .iter()
        .filter(|(source, _, edge)| source == user_id && *edge == edge_type)
        .map(|(_, target, _)| target.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ordered_social_pair;

    #[test]
    fn social_pair_lock_has_one_key_for_both_directions() {
        assert_eq!(
            ordered_social_pair("user-a", "user-b"),
            ordered_social_pair("user-b", "user-a")
        );
    }
}
