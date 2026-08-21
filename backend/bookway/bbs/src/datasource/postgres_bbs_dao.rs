use super::*;

pub(crate) struct PostgresBbsDao {
    pool: sqlx::PgPool,
}

impl PostgresBbsDao {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BbsDao for PostgresBbsDao {
    async fn context(&self, user_id: &str) -> Result<pb::SocialContext, DaoError> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT target_user_id, edge_type FROM social_edges WHERE source_user_id = $1 AND deleted_at IS NULL ORDER BY target_user_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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

    async fn visibility_context(&self, user_id: &str) -> Result<pb::SocialVisibility, DaoError> {
        let excluded_author_ids = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT CASE WHEN source_user_id = $1 THEN target_user_id ELSE source_user_id END FROM social_edges WHERE deleted_at IS NULL AND ((source_user_id = $1 AND edge_type IN ('block', 'mute')) OR (target_user_id = $1 AND edge_type = 'block')) ORDER BY 1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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
    ) -> Result<pb::SocialContext, DaoError> {
        let edge_type = edge_name(edge);
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        let (first_user_id, second_user_id) = ordered_social_pair(user_id, target_user_id);
        // A block removes follows in both directions. Serialize every mutation
        // for this user pair so a concurrent follow cannot commit after that cleanup.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(first_user_id)
            .bind(second_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        if active && edge == pb::SocialEdgeType::Follow {
            let blocked = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM social_edges WHERE edge_type = 'block' AND deleted_at IS NULL AND ((source_user_id = $1 AND target_user_id = $2) OR (source_user_id = $2 AND target_user_id = $1)))",
            )
            .bind(user_id)
            .bind(target_user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
            if blocked {
                return Err(DaoError::BlockedRelationship);
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
            .map_err(DaoError::Database)?;
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
            .map_err(DaoError::Database)?;
        } else {
            sqlx::query(
                "UPDATE social_edges SET deleted_at = now() WHERE source_user_id = $1 AND target_user_id = $2 AND edge_type = $3 AND deleted_at IS NULL",
            )
            .bind(user_id)
            .bind(target_user_id)
            .bind(edge_type)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
        }
        transaction.commit().await.map_err(DaoError::Database)?;
        self.context(user_id).await
    }

    async fn list_route_participations(
        &self,
        user_id: &str,
    ) -> Result<Vec<pb::RouteParticipation>, DaoError> {
        let rows = sqlx::query_as::<_, (String, Option<String>, time::OffsetDateTime)>(
            "SELECT route_id, private_journey_id, joined_at FROM route_participations WHERE user_id = $1 AND left_at IS NULL ORDER BY joined_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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
    ) -> Result<pb::RouteParticipationContext, DaoError> {
        if route_ids.is_empty() {
            return Ok(pb::RouteParticipationContext::default());
        }
        let counts = sqlx::query_as::<_, (String, i64)>(
            "SELECT route_id, SUM(active_count)::BIGINT FROM route_participation_count_shards WHERE route_id = ANY($1) GROUP BY route_id",
        )
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
        let joined_route_ids = sqlx::query_scalar::<_, String>(
            "SELECT route_id FROM route_participations WHERE user_id = $1 AND route_id = ANY($2) AND left_at IS NULL ORDER BY route_id",
        )
        .bind(user_id)
        .bind(route_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(DaoError::Database)?;
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
    ) -> Result<pb::RouteParticipationState, DaoError> {
        let mut transaction = self.pool.begin().await.map_err(DaoError::Database)?;
        // Only commands for the same user and route need ordering. Hot routes can use all shards.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::TEXT), hashtext($2::TEXT))")
            .bind(user_id)
            .bind(route_id)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;

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
            .map_err(DaoError::Database)?;
        } else {
            sqlx::query(
                "INSERT INTO route_participations (route_id, user_id, private_journey_id, left_at, last_intent_version) VALUES ($1, $2, NULL, now(), COALESCE($3, 0)) ON CONFLICT (route_id, user_id) DO UPDATE SET private_journey_id = NULL, left_at = CASE WHEN route_participations.left_at IS NULL THEN now() ELSE route_participations.left_at END, last_intent_version = COALESCE($3, route_participations.last_intent_version) WHERE ($3 IS NOT NULL AND $3 >= route_participations.last_intent_version) OR ($3 IS NULL AND route_participations.last_intent_version = 0)",
            )
            .bind(route_id)
            .bind(user_id)
            .bind(intent_version)
            .execute(&mut *transaction)
            .await
            .map_err(DaoError::Database)?;
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
        .map_err(DaoError::Database)?;
        let joined = left_at.is_none();
        let participant_count = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(active_count), 0)::BIGINT FROM route_participation_count_shards WHERE route_id = $1",
        )
        .bind(route_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(DaoError::Database)?;
        transaction.commit().await.map_err(DaoError::Database)?;
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
