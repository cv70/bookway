use std::{
    collections::{BTreeMap, BTreeSet},
    env,
};

use serde::Serialize;
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const TOP_K_NDCG: i32 = 5;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("SEARCH_EVAL_START_AT must be before SEARCH_EVAL_CUTOFF_AT")]
    InvalidRange,
}

#[derive(Clone, Debug)]
struct Config {
    data_start_at: OffsetDateTime,
    data_cutoff_at: OffsetDateTime,
    label_window_hours: i32,
    min_rendered_items: i64,
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let label_window_hours =
            env_number("SEARCH_EVAL_LABEL_WINDOW_HOURS", 168_i32)?.clamp(1, 720);
        let data_cutoff_at =
            timestamp_from_env("SEARCH_EVAL_CUTOFF_AT")?.unwrap_or_else(OffsetDateTime::now_utc);
        let data_start_at = timestamp_from_env("SEARCH_EVAL_START_AT")?
            .unwrap_or_else(|| data_cutoff_at - Duration::days(7));
        if data_start_at >= data_cutoff_at {
            return Err(ConfigError::InvalidRange);
        }
        Ok(Self {
            data_start_at,
            data_cutoff_at,
            label_window_hours,
            min_rendered_items: env_number("SEARCH_EVAL_MIN_RENDERED_ITEMS", 500_i64)?
                .clamp(1, 100_000_000),
        })
    }
}

fn env_number<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    match env::var(key) {
        Ok(value) => value
            .parse()
            .map_err(|_| ConfigError::Invalid { key, value }),
        Err(_) => Ok(default),
    }
}

fn timestamp_from_env(key: &'static str) -> Result<Option<OffsetDateTime>, ConfigError> {
    match env::var(key) {
        Ok(value) => OffsetDateTime::parse(value.trim(), &Rfc3339)
            .map(Some)
            .map_err(|_| ConfigError::Invalid { key, value }),
        Err(_) => Ok(None),
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct EvaluationItem {
    request_id: String,
    query_rewrite_version: String,
    result_type: String,
    position: i32,
    rendered: bool,
    clicked: bool,
    viewed: bool,
    high_intent: bool,
    negative: bool,
    reward: f64,
    penalty: f64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct GroupKey {
    query_rewrite_version: String,
    result_type: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum EvaluationStatus {
    Ready,
    InsufficientData,
}

impl EvaluationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::InsufficientData => "insufficient_data",
        }
    }
}

#[derive(Debug, Serialize)]
struct EvaluationMetrics {
    schema_version: u8,
    eligible_requests: u64,
    served_items: u64,
    rendered_items: u64,
    groups: Vec<GroupMetrics>,
}

#[derive(Debug, Serialize)]
struct GroupMetrics {
    query_rewrite_version: String,
    result_type: String,
    requests: u64,
    rendered_requests: u64,
    served_items: u64,
    rendered_items: u64,
    render_rate: Option<f64>,
    click_through_rate: Option<f64>,
    view_rate: Option<f64>,
    high_intent_rate: Option<f64>,
    negative_feedback_rate: Option<f64>,
    mean_net_utility: Option<f64>,
    visible_utility_ndcg_at_5: Option<f64>,
    ndcg_sampled_requests: u64,
}

#[derive(Debug, Serialize)]
struct EvaluationRun {
    id: String,
    status: EvaluationStatus,
    data_start_at: String,
    data_cutoff_at: String,
    label_window_hours: i32,
    min_rendered_items: i64,
    metrics: EvaluationMetrics,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bookway_runtime::init_tracing("search-evaluator");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let metrics = build_metrics(&load_evaluation_items(&pool, &config).await?);
    let status = evaluation_status(metrics.rendered_items, config.min_rendered_items);
    let run_id = Uuid::now_v7();
    record_run(&pool, run_id, &config, status, &metrics).await?;

    let run = EvaluationRun {
        id: run_id.to_string(),
        status,
        data_start_at: format_timestamp(config.data_start_at),
        data_cutoff_at: format_timestamp(config.data_cutoff_at),
        label_window_hours: config.label_window_hours,
        min_rendered_items: config.min_rendered_items,
        metrics,
    };
    tracing::info!(
        run_id = %run.id,
        status = status.as_str(),
        served_items = run.metrics.served_items,
        rendered_items = run.metrics.rendered_items,
        "search evaluation snapshot recorded"
    );
    println!("{}", serde_json::to_string(&run)?);
    Ok(())
}

async fn load_evaluation_items(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<Vec<EvaluationItem>, sqlx::Error> {
    // Search Main persists request IDs only for served results. User Event only
    // keeps a matching request ID after validating user, session, result, and
    // position, so this joins high-confidence feedback without query text.
    sqlx::query_as::<_, EvaluationItem>(
        r#"
        WITH eligible_exposures AS (
            SELECT
                request_id,
                user_id,
                query_rewrite_version,
                created_at
            FROM search_exposures
            WHERE NOT degraded
              AND created_at >= $1
              AND created_at < $2 - make_interval(hours => $3::integer)
        )
        SELECT
            exposure.request_id,
            exposure.query_rewrite_version,
            item.result_type,
            item.position,
            COALESCE(BOOL_OR(event.event_type = 'impression'), false) AS rendered,
            COALESCE(BOOL_OR(event.event_type = 'click'), false) AS clicked,
            COALESCE(BOOL_OR(event.event_type = 'view'), false) AS viewed,
            COALESCE(BOOL_OR(event.event_type IN ('like', 'bookmark', 'save_knowledge', 'share', 'join_route', 'complete')), false) AS high_intent,
            COALESCE(BOOL_OR(event.event_type IN ('hide', 'report')), false) AS negative,
            COALESCE(MAX(CASE event.event_type
                WHEN 'complete' THEN 5.0
                WHEN 'join_route' THEN 5.0
                WHEN 'save_knowledge' THEN 4.0
                WHEN 'bookmark' THEN 3.0
                WHEN 'share' THEN 2.5
                WHEN 'like' THEN 2.0
                WHEN 'click' THEN 1.0
                WHEN 'view' THEN 0.4
                ELSE 0.0
            END), 0.0)::double precision AS reward,
            COALESCE(MAX(CASE
                WHEN event.event_type = 'report' THEN 8.0
                WHEN event.event_type = 'hide'
                    AND event.negative_feedback_reason = 'already_seen' THEN 0.25
                WHEN event.event_type = 'hide' THEN 5.0
                ELSE 0.0
            END), 0.0)::double precision AS penalty
        FROM eligible_exposures AS exposure
        INNER JOIN search_exposure_items AS item ON item.request_id = exposure.request_id
        LEFT JOIN user_events AS event
            ON event.request_id = exposure.request_id
           AND event.user_id = exposure.user_id
           AND event.content_id = item.result_id
           AND event.position = item.position
           AND event.received_at >= exposure.created_at
           AND event.received_at < exposure.created_at
               + make_interval(hours => $3::integer)
        GROUP BY
            exposure.request_id,
            exposure.query_rewrite_version,
            item.result_type,
            item.position
        ORDER BY
            exposure.query_rewrite_version,
            item.result_type,
            exposure.request_id,
            item.position
        "#,
    )
    .bind(config.data_start_at)
    .bind(config.data_cutoff_at)
    .bind(config.label_window_hours)
    .fetch_all(pool)
    .await
}

fn build_metrics(items: &[EvaluationItem]) -> EvaluationMetrics {
    let mut groups = BTreeMap::<GroupKey, Vec<&EvaluationItem>>::new();
    for item in items {
        groups
            .entry(GroupKey {
                query_rewrite_version: item.query_rewrite_version.clone(),
                result_type: item.result_type.clone(),
            })
            .or_default()
            .push(item);
    }
    let groups = groups
        .into_iter()
        .map(|(key, items)| group_metrics(key, items))
        .collect::<Vec<_>>();
    EvaluationMetrics {
        schema_version: 1,
        eligible_requests: count(
            items
                .iter()
                .map(|item| item.request_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
        ),
        served_items: groups.iter().map(|group| group.served_items).sum(),
        rendered_items: groups.iter().map(|group| group.rendered_items).sum(),
        groups,
    }
}

fn group_metrics(key: GroupKey, items: Vec<&EvaluationItem>) -> GroupMetrics {
    let mut requests = BTreeMap::<&str, Vec<&EvaluationItem>>::new();
    for item in &items {
        requests.entry(&item.request_id).or_default().push(*item);
    }
    let served_items = count(items.len());
    let rendered = items
        .iter()
        .copied()
        .filter(|item| item.rendered)
        .collect::<Vec<_>>();
    let rendered_items = count(rendered.len());
    let clicked = count(rendered.iter().filter(|item| item.clicked).count());
    let viewed = count(rendered.iter().filter(|item| item.viewed).count());
    let high_intent = count(rendered.iter().filter(|item| item.high_intent).count());
    let negative = count(rendered.iter().filter(|item| item.negative).count());
    let mean_net_utility = (!rendered.is_empty()).then(|| {
        rendered
            .iter()
            .map(|item| observed_utility(item))
            .sum::<f64>()
            / rendered.len() as f64
    });
    let (visible_utility_ndcg_at_5, ndcg_sampled_requests) = visible_ndcg(&requests);

    GroupMetrics {
        query_rewrite_version: key.query_rewrite_version,
        result_type: key.result_type,
        requests: count(requests.len()),
        rendered_requests: count(
            requests
                .values()
                .filter(|request| request.iter().any(|item| item.rendered))
                .count(),
        ),
        served_items,
        rendered_items,
        render_rate: ratio(rendered_items, served_items),
        click_through_rate: ratio(clicked, rendered_items),
        view_rate: ratio(viewed, rendered_items),
        high_intent_rate: ratio(high_intent, rendered_items),
        negative_feedback_rate: ratio(negative, rendered_items),
        mean_net_utility,
        visible_utility_ndcg_at_5,
        ndcg_sampled_requests,
    }
}

fn visible_ndcg(requests: &BTreeMap<&str, Vec<&EvaluationItem>>) -> (Option<f64>, u64) {
    let mut total = 0.0;
    let mut sampled = 0_u64;
    for request in requests.values() {
        let mut ideal_gains = request
            .iter()
            .copied()
            .filter(|item| item.rendered && item.position < TOP_K_NDCG)
            .map(relevance_gain)
            .collect::<Vec<_>>();
        ideal_gains.sort_by(|left, right| right.total_cmp(left));
        let ideal = dcg(&ideal_gains);
        if ideal == 0.0 {
            continue;
        }
        let mut served = request
            .iter()
            .copied()
            .filter(|item| item.rendered && item.position < TOP_K_NDCG)
            .collect::<Vec<_>>();
        served.sort_by_key(|item| item.position);
        total += dcg(&served.into_iter().map(relevance_gain).collect::<Vec<_>>()) / ideal;
        sampled = sampled.saturating_add(1);
    }
    (ratio_f64(total, sampled), sampled)
}

fn dcg(gains: &[f64]) -> f64 {
    gains
        .iter()
        .enumerate()
        .map(|(rank, gain)| gain / (rank as f64 + 2.0).log2())
        .sum()
}

fn observed_utility(item: &EvaluationItem) -> f64 {
    item.reward - item.penalty
}

fn relevance_gain(item: &EvaluationItem) -> f64 {
    observed_utility(item).max(0.0)
}

fn evaluation_status(rendered_items: u64, min_rendered_items: i64) -> EvaluationStatus {
    if i64::try_from(rendered_items).unwrap_or(i64::MAX) >= min_rendered_items {
        EvaluationStatus::Ready
    } else {
        EvaluationStatus::InsufficientData
    }
}

async fn record_run(
    pool: &sqlx::PgPool,
    id: Uuid,
    config: &Config,
    status: EvaluationStatus,
    metrics: &EvaluationMetrics,
) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "INSERT INTO search_evaluation_runs (id, data_start_at, data_cutoff_at, label_window_hours, min_rendered_items, status, metrics) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(config.data_start_at)
    .bind(config.data_cutoff_at)
    .bind(config.label_window_hours)
    .bind(config.min_rendered_items)
    .bind(status.as_str())
    .bind(serde_json::to_value(metrics)?)
    .execute(pool)
    .await?;
    Ok(())
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn ratio(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

fn ratio_f64(numerator: f64, denominator: u64) -> Option<f64> {
    (denominator > 0).then(|| numerator / denominator as f64)
}

fn format_timestamp(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        request_id: &str,
        result_type: &str,
        position: i32,
        rendered: bool,
        reward: f64,
        penalty: f64,
    ) -> EvaluationItem {
        EvaluationItem {
            request_id: request_id.to_string(),
            query_rewrite_version: "lifestyle-v2".to_string(),
            result_type: result_type.to_string(),
            position,
            rendered,
            clicked: reward >= 1.0,
            viewed: reward == 0.4,
            high_intent: reward >= 2.0,
            negative: penalty > 0.0,
            reward,
            penalty,
        }
    }

    #[test]
    fn metrics_group_observed_outcomes_by_rewrite_version_and_result_type() {
        let metrics = build_metrics(&[
            item("request-1", "post", 0, true, 1.0, 0.0),
            item("request-1", "post", 1, true, 0.0, 5.0),
            item("request-2", "journey", 0, true, 5.0, 0.0),
        ]);

        assert_eq!(metrics.eligible_requests, 2);
        assert_eq!(metrics.served_items, 3);
        let post = &metrics.groups[0];
        assert_eq!(post.query_rewrite_version, "lifestyle-v2");
        assert_eq!(post.result_type, "journey");
        let journey = &metrics.groups[1];
        assert_eq!(journey.result_type, "post");
        assert_eq!(journey.click_through_rate, Some(0.5));
        assert_eq!(journey.negative_feedback_rate, Some(0.5));
        assert_eq!(journey.mean_net_utility, Some(-2.0));
    }

    #[test]
    fn ndcg_detects_a_reward_that_was_ranked_below_an_irrelevant_item() {
        let metrics = build_metrics(&[
            item("request-1", "post", 0, true, 0.0, 0.0),
            item("request-1", "post", 1, true, 5.0, 0.0),
        ]);

        let ndcg = metrics.groups[0]
            .visible_utility_ndcg_at_5
            .expect("positive visible feedback can be evaluated");
        assert!((ndcg - 1.0 / 3.0_f64.log2()).abs() < f64::EPSILON);
    }

    #[test]
    fn small_samples_are_not_promoted_to_ready() {
        assert!(matches!(
            evaluation_status(499, 500),
            EvaluationStatus::InsufficientData
        ));
        assert!(matches!(
            evaluation_status(500, 500),
            EvaluationStatus::Ready
        ));
    }
}
