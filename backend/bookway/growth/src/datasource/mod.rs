use std::collections::HashMap;

use super::api::{ActionDto, ActionStateDto, GrowthDomainDto, JourneyDto, JourneyStatusDto};
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("action {0} was not found")]
    ActionNotFound(String),
    #[error("database operation failed: {0}")]
    Database(#[source] sqlx::Error),
    #[error("stored growth data is invalid: {0}")]
    Serialization(#[source] serde_json::Error),
}

#[async_trait]
pub(crate) trait GrowthRepository: Send + Sync {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<JourneyDto>, RepositoryError>;
    async fn create_journey(
        &self,
        user_id: &str,
        journey: JourneyDto,
        first_action: ActionDto,
    ) -> Result<JourneyDto, RepositoryError>;
    async fn today(&self, user_id: &str) -> Result<Vec<ActionDto>, RepositoryError>;
    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, RepositoryError>;
}

struct State {
    journeys: Vec<JourneyDto>,
    actions: HashMap<String, ActionDto>,
}

pub(crate) struct MemoryGrowthRepository {
    state: RwLock<State>,
}

impl MemoryGrowthRepository {
    pub(crate) fn seeded() -> Self {
        let journeys = vec![
            JourneyDto {
                id: "journey-reading".to_string(),
                title: "读懂现代城市".to_string(),
                intent: "用阅读建立观察一座城市的方法".to_string(),
                domain: GrowthDomainDto::Learning,
                status: JourneyStatusDto::Active,
                progress: 36,
                duration_label: "6 周".to_string(),
                next_action: "阅读《看不见的城市》第三章".to_string(),
                participant_count: 1284,
            },
            JourneyDto {
                id: "journey-running".to_string(),
                title: "重新跑起来".to_string(),
                intent: "以不受伤的方式恢复规律运动".to_string(),
                domain: GrowthDomainDto::Movement,
                status: JourneyStatusDto::Active,
                progress: 58,
                duration_label: "4 周".to_string(),
                next_action: "轻松跑 25 分钟".to_string(),
                participant_count: 3276,
            },
        ];

        let actions = [
            ActionDto {
                id: "action-read-city".to_string(),
                journey_id: "journey-reading".to_string(),
                title: "阅读第三章".to_string(),
                detail: "标记一个关于城市空间的观点".to_string(),
                estimated_minutes: 30,
                scheduled_label: "上午".to_string(),
                state: ActionStateDto::Pending,
            },
            ActionDto {
                id: "action-easy-run".to_string(),
                journey_id: "journey-running".to_string(),
                title: "轻松跑 25 分钟".to_string(),
                detail: "保持可以自然说话的配速".to_string(),
                estimated_minutes: 25,
                scheduled_label: "傍晚".to_string(),
                state: ActionStateDto::Pending,
            },
            ActionDto {
                id: "action-stretch".to_string(),
                journey_id: "journey-running".to_string(),
                title: "跑后拉伸".to_string(),
                detail: "完成小腿与髋部拉伸".to_string(),
                estimated_minutes: 8,
                scheduled_label: "傍晚".to_string(),
                state: ActionStateDto::Completed,
            },
        ]
        .into_iter()
        .map(|action| (action.id.clone(), action))
        .collect();

        Self {
            state: RwLock::new(State { journeys, actions }),
        }
    }
}

#[async_trait]
impl GrowthRepository for MemoryGrowthRepository {
    async fn list_journeys(&self, _user_id: &str) -> Result<Vec<JourneyDto>, RepositoryError> {
        Ok(self.state.read().await.journeys.clone())
    }

    async fn create_journey(
        &self,
        _user_id: &str,
        journey: JourneyDto,
        first_action: ActionDto,
    ) -> Result<JourneyDto, RepositoryError> {
        let mut state = self.state.write().await;
        state.actions.insert(first_action.id.clone(), first_action);
        state.journeys.push(journey.clone());
        Ok(journey)
    }

    async fn today(&self, _user_id: &str) -> Result<Vec<ActionDto>, RepositoryError> {
        let mut actions: Vec<_> = self.state.read().await.actions.values().cloned().collect();
        actions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(actions)
    }

    async fn complete_action(
        &self,
        _user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, RepositoryError> {
        let mut state = self.state.write().await;
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        action.state = ActionStateDto::Completed;
        Ok(action.clone())
    }
}

pub(crate) struct PostgresGrowthRepository {
    pool: sqlx::PgPool,
}

impl PostgresGrowthRepository {
    pub(crate) fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GrowthRepository for PostgresGrowthRepository {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<JourneyDto>, RepositoryError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE user_id = $1 ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect()
    }

    async fn create_journey(
        &self,
        user_id: &str,
        journey: JourneyDto,
        first_action: ActionDto,
    ) -> Result<JourneyDto, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(RepositoryError::Database)?;
        sqlx::query(
            "INSERT INTO journeys (id, user_id, payload, status, progress) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&journey.id)
        .bind(user_id)
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(i32::from(journey.progress))
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        sqlx::query(
            "INSERT INTO actions (id, journey_id, user_id, payload, state) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&first_action.id)
        .bind(&first_action.journey_id)
        .bind(user_id)
        .bind(serde_json::to_value(&first_action).map_err(RepositoryError::Serialization)?)
        .bind(format_action_state(first_action.state))
        .execute(&mut *transaction)
        .await
        .map_err(RepositoryError::Database)?;
        transaction
            .commit()
            .await
            .map_err(RepositoryError::Database)?;
        Ok(journey)
    }

    async fn today(&self, user_id: &str) -> Result<Vec<ActionDto>, RepositoryError> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE user_id = $1 AND scheduled_for = CURRENT_DATE ORDER BY id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        rows.into_iter()
            .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
            .collect()
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, RepositoryError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "UPDATE actions SET state = 'completed', payload = jsonb_set(payload, '{state}', to_jsonb('completed'::text), true), updated_at = now() WHERE id = $1 AND user_id = $2 RETURNING payload",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        serde_json::from_value(payload).map_err(RepositoryError::Serialization)
    }
}

fn format_status(status: JourneyStatusDto) -> &'static str {
    match status {
        JourneyStatusDto::Active => "active",
        JourneyStatusDto::Paused => "paused",
        JourneyStatusDto::Completed => "completed",
    }
}

fn format_action_state(state: ActionStateDto) -> &'static str {
    match state {
        ActionStateDto::Pending => "pending",
        ActionStateDto::Completed => "completed",
        ActionStateDto::Skipped => "skipped",
    }
}
