use std::{collections::BTreeMap, env};

use serde::Serialize;
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const TOP_K_NDCG: i32 = 5;
const TOP_K_CAPTURE: i32 = 3;

#[derive(Debug, Error)]
enum ConfigError {
    #[error("invalid {key}: {value}")]
    Invalid { key: &'static str, value: String },
    #[error("RECOMMEND_EVAL_START_AT must be before RECOMMEND_EVAL_CUTOFF_AT")]
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
            env_number("RECOMMEND_EVAL_LABEL_WINDOW_HOURS", 168_i32)?.clamp(1, 720);
        let data_cutoff_at =
            timestamp_from_env("RECOMMEND_EVAL_CUTOFF_AT")?.unwrap_or_else(OffsetDateTime::now_utc);
        let data_start_at = timestamp_from_env("RECOMMEND_EVAL_START_AT")?
            .unwrap_or_else(|| data_cutoff_at - Duration::days(7));
        if data_start_at >= data_cutoff_at {
            return Err(ConfigError::InvalidRange);
        }
        Ok(Self {
            data_start_at,
            data_cutoff_at,
            label_window_hours,
            min_rendered_items: env_number("RECOMMEND_EVAL_MIN_RENDERED_ITEMS", 1_000_i64)?
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
    surface: String,
    pipeline_id: String,
    model_version: String,
    experiment_bucket: Option<String>,
    position: i32,
    score: f64,
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
    surface: String,
    pipeline_id: String,
    model_version: String,
    experiment_bucket: Option<String>,
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
    surface: String,
    pipeline_id: String,
    model_version: String,
    experiment_bucket: Option<String>,
    requests: u64,
    rendered_requests: u64,
    served_items: u64,
    rendered_items: u64,
    render_rate: Option<f64>,
    click_through_rate: Option<f64>,
    view_rate: Option<f64>,
    high_intent_rate: Option<f64>,
    negative_feedback_rate: Option<f64>,
    mean_rank_score: Option<f64>,
    mean_net_utility: Option<f64>,
    visible_utility_ndcg_at_5: Option<f64>,
    visible_top_3_utility_capture: Option<f64>,
    ndcg_sampled_requests: u64,
    capture_sampled_requests: u64,
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
    bookway_runtime::init_tracing("recommendation-evaluator");
    let config = Config::from_env()?;
    let pool = bookway_data::postgres_pool().await?;
    let items = load_evaluation_items(&pool, &config).await?;
    let metrics = build_metrics(&items);
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
        "recommendation evaluation snapshot recorded"
    );
    println!("{}", serde_json::to_string(&run)?);
    Ok(())
}

async fn load_evaluation_items(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<Vec<EvaluationItem>, sqlx::Error> {
    // Join only the request IDs preserved by User Event after Recommend Main
    // validates the exact user, content, and rank position. `received_at`
    // bounds labels by server observation time, not a mutable device clock.
    sqlx::query_as::<_, EvaluationItem>(
        r#"
        WITH eligible_exposures AS (
            SELECT
                request_id,
                user_id,
                surface,
                pipeline_id,
                COALESCE(NULLIF(model_version, ''), 'unversioned') AS model_version,
                NULLIF(experiment_bucket, '') AS experiment_bucket,
                created_at
            FROM feed_exposures
            WHERE user_id IS NOT NULL
              AND NOT degraded
              AND created_at >= $1
              AND created_at < $2 - make_interval(hours => $3::integer)
        )
        SELECT
            exposure.request_id,
            exposure.surface,
            exposure.pipeline_id,
            exposure.model_version,
            exposure.experiment_bucket,
            item.position,
            item.score,
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
                WHEN 'click' THEN 0.6
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
        INNER JOIN feed_exposure_items AS item ON item.request_id = exposure.request_id
        LEFT JOIN user_events AS event
            ON event.request_id = exposure.request_id
           AND event.user_id = exposure.user_id
           AND event.content_id = item.content_id
           AND event.position = item.position
           AND event.received_at >= exposure.created_at
           AND event.received_at < exposure.created_at
               + make_interval(hours => $3::integer)
        GROUP BY
            exposure.request_id,
            exposure.surface,
            exposure.pipeline_id,
            exposure.model_version,
            exposure.experiment_bucket,
            item.position,
            item.score
        ORDER BY
            exposure.surface,
            exposure.pipeline_id,
            exposure.model_version,
            exposure.experiment_bucket,
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
    let mut grouped = BTreeMap::<GroupKey, Vec<&EvaluationItem>>::new();
    for item in items {
        grouped
            .entry(GroupKey {
                surface: item.surface.clone(),
                pipeline_id: item.pipeline_id.clone(),
                model_version: item.model_version.clone(),
                experiment_bucket: item.experiment_bucket.clone(),
            })
            .or_default()
            .push(item);
    }

    let groups = grouped
        .into_iter()
        .map(|(key, items)| group_metrics(key, items))
        .collect::<Vec<_>>();
    let eligible_requests = groups.iter().map(|group| group.requests).sum();
    let served_items = groups.iter().map(|group| group.served_items).sum();
    let rendered_items = groups.iter().map(|group| group.rendered_items).sum();
    EvaluationMetrics {
        schema_version: 1,
        eligible_requests,
        served_items,
        rendered_items,
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
    let finite_scores = items
        .iter()
        .filter_map(|item| item.score.is_finite().then_some(item.score))
        .collect::<Vec<_>>();
    let mean_rank_score = (!finite_scores.is_empty())
        .then(|| finite_scores.iter().sum::<f64>() / usize_to_f64(finite_scores.len()));
    let mean_net_utility = (!rendered.is_empty()).then(|| {
        rendered
            .iter()
            .map(|item| observed_utility(item))
            .sum::<f64>()
            / usize_to_f64(rendered.len())
    });
    let (visible_utility_ndcg_at_5, ndcg_sampled_requests) = visible_ndcg(&requests);
    let (visible_top_3_utility_capture, capture_sampled_requests) = visible_capture(&requests);

    GroupMetrics {
        surface: key.surface,
        pipeline_id: key.pipeline_id,
        model_version: key.model_version,
        experiment_bucket: key.experiment_bucket,
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
        mean_rank_score,
        mean_net_utility,
        visible_utility_ndcg_at_5,
        visible_top_3_utility_capture,
        ndcg_sampled_requests,
        capture_sampled_requests,
    }
}

fn visible_ndcg(requests: &BTreeMap<&str, Vec<&EvaluationItem>>) -> (Option<f64>, u64) {
    let mut total = 0.0;
    let mut sampled = 0_u64;
    for request in requests.values() {
        let mut observed = request
            .iter()
            .copied()
            .filter(|item| item.rendered && item.position < TOP_K_NDCG)
            .map(relevance_gain)
            .collect::<Vec<_>>();
        observed.sort_by(|left, right| right.total_cmp(left));
        let ideal = dcg(&observed);
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

fn visible_capture(requests: &BTreeMap<&str, Vec<&EvaluationItem>>) -> (Option<f64>, u64) {
    let mut total = 0.0;
    let mut sampled = 0_u64;
    for request in requests.values() {
        let visible = request
            .iter()
            .copied()
            .filter(|item| item.rendered)
            .collect::<Vec<_>>();
        let available = visible.iter().map(|item| relevance_gain(item)).sum::<f64>();
        if available == 0.0 {
            continue;
        }
        let captured = visible
            .iter()
            .filter(|item| item.position < TOP_K_CAPTURE)
            .map(|item| relevance_gain(item))
            .sum::<f64>();
        total += captured / available;
        sampled = sampled.saturating_add(1);
    }
    (ratio_f64(total, sampled), sampled)
}

fn dcg(gains: &[f64]) -> f64 {
    gains
        .iter()
        .enumerate()
        .map(|(rank, gain)| gain / (usize_to_f64(rank) + 2.0).log2())
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
    let metrics = serde_json::to_value(metrics)?;
    sqlx::query(
        "INSERT INTO recommendation_evaluation_runs (id, data_start_at, data_cutoff_at, label_window_hours, min_rendered_items, status, metrics) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(config.data_start_at)
    .bind(config.data_cutoff_at)
    .bind(config.label_window_hours)
    .bind(config.min_rendered_items)
    .bind(status.as_str())
    .bind(metrics)
    .execute(pool)
    .await?;
    Ok(())
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn usize_to_f64(value: usize) -> f64 {
    value as f64
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
        position: i32,
        rendered: bool,
        reward: f64,
        penalty: f64,
    ) -> EvaluationItem {
        EvaluationItem {
            request_id: request_id.to_string(),
            surface: "home".to_string(),
            pipeline_id: "bookway-recommend-main-home".to_string(),
            model_version: "rank-v2".to_string(),
            experiment_bucket: Some("rank-v2-3".to_string()),
            position,
            score: 1.0 - f64::from(position) * 0.1,
            rendered,
            clicked: reward >= 0.6,
            viewed: false,
            high_intent: reward >= 2.0,
            negative: penalty > 0.0,
            reward,
            penalty,
        }
    }

    #[test]
    fn metrics_group_verified_visible_feedback_by_rank_identity() {
        let report = build_metrics(&[
            item("request-1", 0, true, 3.0, 0.0),
            item("request-1", 1, true, 0.0, 5.0),
            item("request-2", 0, false, 5.0, 0.0),
        ]);

        assert_eq!(report.eligible_requests, 2);
        assert_eq!(report.served_items, 3);
        assert_eq!(report.rendered_items, 2);
        let group = &report.groups[0];
        assert_eq!(group.model_version, "rank-v2");
        assert_eq!(group.requests, 2);
        assert_eq!(group.rendered_requests, 1);
        assert_eq!(group.click_through_rate, Some(0.5));
        assert_eq!(group.high_intent_rate, Some(0.5));
        assert_eq!(group.negative_feedback_rate, Some(0.5));
        assert_eq!(group.mean_net_utility, Some(-1.0));
    }

    #[test]
    fn visible_ndcg_penalizes_a_reward_below_a_zero_gain_item() {
        let report = build_metrics(&[
            item("request-1", 0, true, 0.0, 0.0),
            item("request-1", 1, true, 5.0, 0.0),
        ]);

        let ndcg = report.groups[0]
            .visible_utility_ndcg_at_5
            .expect("a visible positive label should be evaluable");
        assert!((ndcg - 1.0 / 3.0_f64.log2()).abs() < f64::EPSILON);
        assert_eq!(report.groups[0].ndcg_sampled_requests, 1);
    }

    #[test]
    fn small_samples_are_explicitly_not_ready() {
        assert!(matches!(
            evaluation_status(999, 1_000),
            EvaluationStatus::InsufficientData
        ));
        assert!(matches!(
            evaluation_status(1_000, 1_000),
            EvaluationStatus::Ready
        ));
    }
}
