use std::env;

use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[derive(Clone, Debug)]
struct Config {
    version: String,
    as_of: OffsetDateTime,
    window_start: OffsetDateTime,
    window_end: OffsetDateTime,
    expires_at: OffsetDateTime,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let as_of = match env::var("FEATURE_SNAPSHOT_AS_OF") {
            Ok(value) => OffsetDateTime::parse(value.trim(), &Rfc3339)
                .map_err(|_| "FEATURE_SNAPSHOT_AS_OF must be RFC3339".to_string())?,
            Err(_) => OffsetDateTime::now_utc(),
        };
        let window_days = env_number("FEATURE_SNAPSHOT_WINDOW_DAYS", 90_i64)?.clamp(1, 730);
        let ttl_days = env_number("FEATURE_SNAPSHOT_TTL_DAYS", 14_i64)?.clamp(1, 90);
        let window_start = as_of - Duration::days(window_days);
        let expires_at = as_of + Duration::days(ttl_days);
        let version = env::var("FEATURE_SNAPSHOT_VERSION")
            .or_else(|_| env::var("FEATURE_MODEL_VERSION"))
            .unwrap_or_else(|_| "heuristic-v1".to_string())
            .trim()
            .to_string();
        if version.is_empty() || version.len() > 64 {
            return Err("FEATURE_SNAPSHOT_VERSION must contain 1-64 characters".to_string());
        }
        Ok(Self {
            version,
            as_of,
            window_start,
            window_end: as_of,
            expires_at,
        })
    }
}

fn env_number<T>(key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value.parse().map_err(|_| format!("{key} must be numeric")),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("feature-snapshot");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let users = load_user_features(&pool, &config).await?;
    let mut transaction = pool.begin().await?;
    for (user_id, features) in &users {
        write_snapshot(&mut transaction, &config, user_id, features).await?;
    }
    transaction.commit().await?;
    cleanup_expired(&pool, config.as_of).await?;
    tracing::info!(users = users.len(), version = %config.version, "feature snapshot batch recorded");
    println!(
        "{}",
        serde_json::to_string(&json!({
            "feature_version": config.version,
            "as_of": format_time(config.as_of),
            "window_start": format_time(config.window_start),
            "window_end": format_time(config.window_end),
            "expires_at": format_time(config.expires_at),
            "users": users.len(),
        }))?
    );
    Ok(())
}

async fn load_user_features(
    pool: &PgPool,
    config: &Config,
) -> Result<Vec<(String, Value)>, sqlx::Error> {
    sqlx::query_as::<_, (String, Value)>(
        r#"
        WITH recent AS (
            SELECT
                event.user_id,
                event.event_type,
                event.negative_feedback_reason,
                content.domain,
                CASE event.event_type
                    WHEN 'complete' THEN 5.0
                    WHEN 'join_route' THEN 5.0
                    WHEN 'save_knowledge' THEN 4.0
                    WHEN 'bookmark' THEN 3.0
                    WHEN 'share' THEN 2.5
                    WHEN 'like' THEN 2.0
                    WHEN 'click' THEN 0.6
                    WHEN 'view' THEN 0.4
                    ELSE 0.0
                END::double precision AS positive_weight,
                CASE
                    WHEN event.event_type = 'report' THEN 8.0
                    WHEN event.event_type = 'hide'
                         AND event.negative_feedback_reason = 'already_seen' THEN 0.25
                    WHEN event.event_type = 'hide' THEN 5.0
                    ELSE 0.0
                END::double precision AS negative_weight,
                CASE WHEN event.event_type IN ('impression', 'view') THEN 1.0 ELSE 0.0 END AS impression_weight
            FROM user_events AS event
            LEFT JOIN content_items AS content ON content.id = event.content_id
            WHERE event.occurred_at >= $1
              AND event.occurred_at < $2
        ),
        user_rollup AS (
            SELECT
                user_id,
                SUM(positive_weight)::double precision AS positive_weight,
                SUM(negative_weight)::double precision AS negative_weight,
                SUM(impression_weight)::double precision AS impressions,
                COUNT(*)::double precision AS event_count
            FROM recent
            GROUP BY user_id
        ),
        domain_scores AS (
            SELECT user_id, domain, SUM(positive_weight - negative_weight)::double precision AS score
            FROM recent
            WHERE domain IS NOT NULL
              AND domain IN ('learning', 'movement', 'wellness', 'travel', 'leisure')
            GROUP BY user_id, domain
        ),
        domain_max AS (
            SELECT user_id, GREATEST(MAX(score), 1.0)::double precision AS maximum
            FROM domain_scores
            GROUP BY user_id
        ),
        domain_features AS (
            SELECT
                scores.user_id,
                jsonb_object_agg(
                    'domain_interest.' || scores.domain,
                    LEAST(GREATEST(scores.score, 0.0) / maximum.maximum, 1.0)
                ) AS features
            FROM domain_scores AS scores
            INNER JOIN domain_max AS maximum ON maximum.user_id = scores.user_id
            GROUP BY scores.user_id
        )
        SELECT
            rollup.user_id,
            jsonb_strip_nulls(
                jsonb_build_object(
                    'recent_positive_rate', LEAST(rollup.positive_weight / GREATEST(rollup.impressions, 1.0), 1.0),
                    'negative_feedback_rate', LEAST(rollup.negative_weight / GREATEST(rollup.impressions, 1.0), 1.0),
                    'user_interest_strength', LEAST(GREATEST((rollup.positive_weight - rollup.negative_weight * 0.75) / GREATEST(rollup.event_count, 1.0), 0.0), 1.0),
                    'snapshot_event_count', rollup.event_count
                ) || COALESCE(domain_features.features, '{}'::jsonb)
            ) AS features
        FROM user_rollup AS rollup
        LEFT JOIN domain_features ON domain_features.user_id = rollup.user_id
        ORDER BY rollup.user_id
        "#,
    )
    .bind(config.window_start)
    .bind(config.window_end)
    .fetch_all(pool)
    .await
}

async fn write_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
    user_id: &str,
    features: &Value,
) -> Result<(), sqlx::Error> {
    let lineage = json!({
        "source": "user_events",
        "schema_version": 1,
        "event_window": {
            "start": format_time(config.window_start),
            "end": format_time(config.window_end),
        },
    });
    sqlx::query(
        "INSERT INTO user_feature_snapshots (snapshot_id,user_id,feature_version,as_of,window_start,window_end,expires_at,features,lineage) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (user_id,feature_version,as_of) DO UPDATE SET window_start=EXCLUDED.window_start,window_end=EXCLUDED.window_end,expires_at=EXCLUDED.expires_at,features=EXCLUDED.features,lineage=EXCLUDED.lineage,created_at=now()",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&config.version)
    .bind(config.as_of)
    .bind(config.window_start)
    .bind(config.window_end)
    .bind(config.expires_at)
    .bind(features)
    .bind(lineage)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn cleanup_expired(pool: &PgPool, as_of: OffsetDateTime) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_feature_snapshots WHERE expires_at <= $1")
        .bind(as_of)
        .execute(pool)
        .await?;
    Ok(())
}

fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}
