use super::*;

#[derive(Clone)]
pub(crate) struct FeatureDao {
    pool: Option<sqlx::PgPool>,
    feature_version: String,
}

impl FeatureDao {
    pub(crate) fn new(pool: Option<sqlx::PgPool>, feature_version: String) -> Self {
        Self {
            pool,
            feature_version,
        }
    }
    pub(crate) async fn load(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        let mut derived = self.load_snapshot(user_id).await;
        // Keep feature freshness bounded while deriving feedback features
        // from the canonical event log. The event types are intentionally
        // weighted so a repeat dismissal does not look like topic rejection,
        // while relevance and safety feedback can meaningfully reduce exploration.
        let feedback = sqlx::query_as::<_, (i64, f64, i64, i64)>(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE event_type IN ('like', 'bookmark', 'save_knowledge', 'share', 'complete')),
                COALESCE(SUM(CASE
                    WHEN event_type = 'hide' AND negative_feedback_reason = 'already_seen' THEN 0.25
                    WHEN event_type IN ('hide', 'report') THEN 1.0
                    ELSE 0.0
                END), 0)::double precision,
                COUNT(*) FILTER (WHERE event_type IN ('impression', 'view')),
                COUNT(*)
            FROM user_events
            WHERE user_id = $1 AND occurred_at > now() - interval '30 days'
            "#,
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or((0, 0.0, 0, 0));
        let positive = feedback.0 as f64;
        let negative = feedback.1;
        let impressions = feedback.2 as f64;
        let total = feedback.3 as f64;
        derived.extend([
            (
                "recent_positive_rate".to_string(),
                (positive / impressions.max(1.0)).min(1.0),
            ),
            (
                "negative_feedback_rate".to_string(),
                (negative / impressions.max(1.0)).min(1.0),
            ),
            (
                "user_interest_strength".to_string(),
                ((positive - negative * 0.75) / total.max(1.0)).clamp(0.0, 1.0),
            ),
        ]);
        derived.extend(self.load_domain_interests(user_id).await);
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT feature_name,value FROM user_features WHERE user_id=$1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => {
                derived.extend(
                    rows.into_iter()
                        .filter_map(|(name, value)| value.as_f64().map(|number| (name, number))),
                );
                derived
            }
            Err(error) => {
                tracing::warn!(%error, user_id, "feature store degraded");
                derived
            }
        }
    }

    async fn load_snapshot(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT features FROM user_feature_snapshots WHERE user_id=$1 AND feature_version=$2 AND expires_at > now() ORDER BY as_of DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(&self.feature_version)
        .fetch_optional(pool)
        .await;
        match row {
            Ok(Some((features,))) => finite_features(features),
            Ok(None) => HashMap::new(),
            Err(error) => {
                tracing::warn!(%error, user_id, version = %self.feature_version, "feature snapshot read degraded");
                HashMap::new()
            }
        }
    }

    async fn load_domain_interests(&self, user_id: &str) -> HashMap<String, f64> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        // These are user-level features, used before candidate generation so
        // a strong interest can expand recall instead of only reranking it.
        let rows = sqlx::query_as::<_, (String, f64)>(
            r#"
            SELECT
                content.domain,
                SUM(
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason IN ('already_seen', 'low_quality') THEN 0.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END
                )::double precision
            FROM user_events AS event
            INNER JOIN content_items AS content ON content.id = event.content_id
            WHERE event.user_id = $1
              AND event.occurred_at > now() - interval '90 days'
            GROUP BY content.domain
            "#,
        )
        .bind(user_id)
        .fetch_all(pool)
        .await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, user_id, "domain interest features degraded");
                return HashMap::new();
            }
        };
        let maximum = rows
            .iter()
            .map(|(_, score)| score.max(0.0))
            .fold(1.0_f64, f64::max);
        rows.into_iter()
            .filter(|(domain, _)| {
                matches!(
                    domain.as_str(),
                    "learning" | "movement" | "wellness" | "travel" | "leisure"
                )
            })
            .filter_map(|(domain, score)| {
                let score = (score.max(0.0) / maximum).clamp(0.0, 1.0);
                (score > 0.0).then(|| (format!("domain_interest.{domain}"), score))
            })
            .collect()
    }

    pub(crate) async fn load_candidates(
        &self,
        user_id: &str,
        content_ids: &[String],
    ) -> HashMap<String, CandidateFeatures> {
        let Some(pool) = &self.pool else {
            return HashMap::new();
        };
        if content_ids.is_empty() {
            return HashMap::new();
        }

        // Normalize high-intent history within each user's strongest domain
        // and author so global popularity does not erase personal preference.
        let rows = sqlx::query_as::<_, (String, f64, f64, f64, f64, f64, f64, f64, f64, f64)>(
            r#"
            WITH history AS (
                SELECT
                    content.domain,
                    content.author_id,
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason IN ('already_seen', 'low_quality') THEN 0.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END AS domain_weight,
                    CASE event.event_type
                        WHEN 'join_route' THEN 5.0
                        WHEN 'complete' THEN 5.0
                        WHEN 'save_knowledge' THEN 4.0
                        WHEN 'bookmark' THEN 3.0
                        WHEN 'share' THEN 2.5
                        WHEN 'like' THEN 2.0
                        WHEN 'click' THEN 0.6
                        WHEN 'view' THEN 0.4
                        WHEN 'hide' THEN CASE
                            WHEN event.negative_feedback_reason = 'already_seen' THEN 0.0
                            WHEN event.negative_feedback_reason = 'not_relevant' THEN 0.0
                            WHEN event.negative_feedback_reason = 'low_quality' THEN -4.0
                            ELSE -5.0
                        END
                        WHEN 'report' THEN -8.0
                        ELSE 0.0
                    END AS author_weight
                FROM user_events AS event
                INNER JOIN content_items AS content ON content.id = event.content_id
                WHERE event.user_id = $1
                  AND event.occurred_at > now() - interval '90 days'
            ),
            domain_scores AS (
                SELECT domain, SUM(domain_weight)::double precision AS score
                FROM history
                GROUP BY domain
            ),
            author_scores AS (
                SELECT author_id, SUM(author_weight)::double precision AS score
                FROM history
                GROUP BY author_id
            ),
            normalizers AS (
                SELECT
                    GREATEST(COALESCE((SELECT MAX(score) FROM domain_scores), 0.0), 1.0) AS domain_max,
                    GREATEST(COALESCE((SELECT MAX(score) FROM author_scores), 0.0), 1.0) AS author_max
            ),
            direct_feedback AS (
                SELECT
                    content_id,
                    COUNT(*) FILTER (
                        WHERE event_type = 'impression'
                          AND occurred_at > now() - interval '30 days'
                    )::double precision AS impression_count,
                    SUM(CASE
                        WHEN event_type = 'hide'
                             AND negative_feedback_reason = 'already_seen'
                             AND occurred_at > now() - interval '90 days' THEN 0.25
                        WHEN event_type IN ('hide', 'report')
                             AND occurred_at > now() - interval '90 days' THEN 1.0
                        ELSE 0.0
                    END)::double precision AS negative_weight
                    ,COUNT(*) FILTER (
                        WHERE event_type = 'click'
                          AND occurred_at > now() - interval '30 days'
                    )::double precision AS clicks
                    ,COUNT(*) FILTER (WHERE event_type IN ('bookmark', 'save_knowledge'))::double precision AS saves
                    ,COUNT(*) FILTER (WHERE event_type = 'save_knowledge')::double precision AS knowledge_starts
                    ,COUNT(*) FILTER (WHERE event_type = 'complete')::double precision AS completions
                    ,COUNT(*) FILTER (WHERE event_type = 'join_route')::double precision AS joins
                    ,COUNT(*) FILTER (WHERE event_type = 'purchase')::double precision AS purchases
                FROM user_events
                WHERE user_id = $1
                  AND content_id = ANY($2)
                  AND occurred_at > now() - interval '90 days'
                GROUP BY content_id
            ),
            population_feedback AS (
                -- Population signals provide a cold-start prior for pCTR,
                -- pCVR and route completion. Personal signals take over only
                -- after enough observations to avoid one-event overfitting.
                SELECT
                    event.content_id,
                    COUNT(*) FILTER (
                        WHERE event_type = 'impression'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS impression_count,
                    COUNT(*) FILTER (
                        WHERE event_type = 'click'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS clicks,
                    COUNT(*) FILTER (
                        WHERE event_type = 'complete'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS completions,
                    COUNT(*) FILTER (
                        WHERE event_type = 'save_knowledge'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS knowledge_starts,
                    COUNT(*) FILTER (
                        WHERE event_type = 'join_route'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS joins,
                    COUNT(*) FILTER (
                        WHERE event_type = 'purchase'
                          AND occurred_at > now() - interval '90 days'
                    )::double precision AS purchases,
                    -- A route completion is meaningful only when the same
                    -- user has a Gateway-verified join for this route in the
                    -- same observation window. Counting the two populations
                    -- independently, or accepting client self-reports, lets
                    -- malformed events inflate route quality above its real
                    -- adoption base.
                    COUNT(DISTINCT event.user_id) FILTER (
                        WHERE event.event_type = 'complete'
                          AND event.source = 'gateway-route-completion'
                          AND EXISTS (
                              SELECT 1
                              FROM user_events AS joined
                              WHERE joined.content_id = event.content_id
                                AND joined.user_id = event.user_id
                                AND joined.event_type = 'join_route'
                                AND joined.source = 'gateway-route-join'
                                AND joined.occurred_at > now() - interval '90 days'
                          )
                    )::double precision AS completion_users,
                    COUNT(DISTINCT event.user_id) FILTER (
                        WHERE event.event_type = 'join_route'
                          AND event.source = 'gateway-route-join'
                    )::double precision AS join_users
                FROM user_events AS event
                WHERE event.content_id = ANY($2)
                  AND event.occurred_at > now() - interval '90 days'
                GROUP BY event.content_id
            )
            SELECT
                candidate.id,
                LEAST(GREATEST(COALESCE(domain.score, 0.0), 0.0) / normalizers.domain_max, 1.0)::double precision,
                LEAST(GREATEST(COALESCE(author.score, 0.0), 0.0) / normalizers.author_max, 1.0)::double precision,
                LEAST(COALESCE(feedback.impression_count, 0.0) / 4.0, 1.0)::double precision,
                LEAST(COALESCE(feedback.negative_weight, 0.0), 1.0)::double precision,
                LEAST(
                    COALESCE(feedback.clicks, 0.0)
                        / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0)
                        * LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)
                    + (COALESCE(population.clicks, 0.0) + 0.5)
                        / GREATEST(COALESCE(population.impression_count, 0.0) + 20.0, 20.0)
                        * (1.0 - LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)),
                    1.0
                )::double precision,
                LEAST(COALESCE(feedback.saves, 0.0) / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0), 1.0)::double precision,
                CASE
                    WHEN candidate.content_type = 'route' THEN LEAST(
                        COALESCE(feedback.completions, 0.0)
                            / GREATEST(COALESCE(feedback.joins, 0.0), 1.0)
                            * LEAST(COALESCE(feedback.joins, 0.0) / 20.0, 1.0)
                        + (COALESCE(population.completions, 0.0) + 0.1)
                            / GREATEST(COALESCE(population.joins, 0.0) + 20.0, 20.0)
                            * (1.0 - LEAST(COALESCE(feedback.joins, 0.0) / 20.0, 1.0)),
                        1.0
                    )
                    ELSE LEAST(
                        COALESCE(feedback.completions, 0.0)
                            / GREATEST(COALESCE(feedback.knowledge_starts, 0.0), 1.0)
                            * LEAST(COALESCE(feedback.knowledge_starts, 0.0) / 20.0, 1.0)
                        + (COALESCE(population.completions, 0.0) + 0.1)
                            / GREATEST(COALESCE(population.knowledge_starts, 0.0) + 20.0, 20.0)
                            * (1.0 - LEAST(COALESCE(feedback.knowledge_starts, 0.0) / 20.0, 1.0)),
                        1.0
                    )
                END::double precision,
                LEAST(
                    CASE
                        WHEN candidate.content_type = 'route' THEN
                            COALESCE(feedback.purchases, 0.0)
                                / GREATEST(COALESCE(feedback.impression_count, 0.0), 1.0)
                                * LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0)
                            + (COALESCE(population.purchases, 0.0) + 0.05)
                                / GREATEST(COALESCE(population.impression_count, 0.0) + 50.0, 50.0)
                                * (1.0 - LEAST(COALESCE(feedback.impression_count, 0.0) / 20.0, 1.0))
                        ELSE 0.0
                    END,
                    1.0
                )::double precision,
                CASE
                    WHEN candidate.content_type = 'route' THEN LEAST(
                        (COALESCE(population.completion_users, 0.0) + 0.1)
                            / GREATEST(COALESCE(population.join_users, 0.0) + 20.0, 20.0),
                        1.0
                    )
                    ELSE 0.0
                END::double precision
            FROM content_items AS candidate
            CROSS JOIN normalizers
            LEFT JOIN domain_scores AS domain ON domain.domain = candidate.domain
            LEFT JOIN author_scores AS author ON author.author_id = candidate.author_id
            LEFT JOIN direct_feedback AS feedback ON feedback.content_id = candidate.id
            LEFT JOIN population_feedback AS population ON population.content_id = candidate.id
            WHERE candidate.id = ANY($2)
            "#,
        )
        .bind(user_id)
        .bind(content_ids)
        .fetch_all(pool)
        .await;

        match rows {
            Ok(rows) => rows
                .into_iter()
                .map(
                    |(
                        content_id,
                        domain_affinity,
                        author_affinity,
                        impression_fatigue,
                        direct_negative_feedback,
                        click_through_rate,
                        save_rate,
                        action_completion_rate,
                        purchase_conversion_rate,
                        route_completion_rate,
                    )| {
                        (
                            content_id,
                            CandidateFeatures {
                                domain_affinity,
                                author_affinity,
                                impression_fatigue,
                                direct_negative_feedback,
                                click_through_rate,
                                save_rate,
                                action_completion_rate,
                                purchase_conversion_rate,
                                route_completion_rate,
                            },
                        )
                    },
                )
                .collect(),
            Err(error) => {
                tracing::warn!(%error, user_id, "candidate feature store degraded");
                HashMap::new()
            }
        }
    }
}
