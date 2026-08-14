use std::collections::HashMap;

use super::api::{
    ActionDto, ActionStateDto, GrowthDomainDto, JourneyDetailDto, JourneyDto, JourneyStatusDto,
    UpdateActionRequest, UpdateJourneyRequest,
};
use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub(crate) enum RepositoryError {
    #[error("journey {0} was not found")]
    JourneyNotFound(String),
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
    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, RepositoryError>;
    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, RepositoryError>;
    async fn create_action(
        &self,
        user_id: &str,
        action: ActionDto,
    ) -> Result<ActionDto, RepositoryError>;
    async fn today(&self, user_id: &str) -> Result<Vec<ActionDto>, RepositoryError>;
    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, RepositoryError>;
    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, RepositoryError>;
}

struct State {
    journeys: Vec<JourneyDto>,
    actions: HashMap<String, ActionDto>,
    journey_owners: HashMap<String, String>,
    action_owners: HashMap<String, String>,
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
        let journey_owners = [
            ("journey-reading".to_string(), "demo-user".to_string()),
            ("journey-running".to_string(), "demo-user".to_string()),
        ]
        .into_iter()
        .collect();
        let action_owners = [
            ("action-read-city".to_string(), "demo-user".to_string()),
            ("action-easy-run".to_string(), "demo-user".to_string()),
            ("action-stretch".to_string(), "demo-user".to_string()),
        ]
        .into_iter()
        .collect();

        Self {
            state: RwLock::new(State {
                journeys,
                actions,
                journey_owners,
                action_owners,
            }),
        }
    }
}

#[async_trait]
impl GrowthRepository for MemoryGrowthRepository {
    async fn list_journeys(&self, user_id: &str) -> Result<Vec<JourneyDto>, RepositoryError> {
        let state = self.state.read().await;
        Ok(state
            .journeys
            .iter()
            .filter(|journey| {
                state
                    .journey_owners
                    .get(&journey.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .cloned()
            .collect())
    }

    async fn create_journey(
        &self,
        user_id: &str,
        journey: JourneyDto,
        first_action: ActionDto,
    ) -> Result<JourneyDto, RepositoryError> {
        let mut state = self.state.write().await;
        state
            .journey_owners
            .insert(journey.id.clone(), user_id.to_string());
        state
            .action_owners
            .insert(first_action.id.clone(), user_id.to_string());
        state.actions.insert(first_action.id.clone(), first_action);
        state.journeys.push(journey.clone());
        Ok(journey)
    }

    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, RepositoryError> {
        let state = self.state.read().await;
        let journey = state
            .journeys
            .iter()
            .find(|journey| {
                journey.id == journey_id
                    && state
                        .journey_owners
                        .get(&journey.id)
                        .is_some_and(|owner| owner == user_id)
            })
            .cloned()
            .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        let mut actions: Vec<_> = state
            .actions
            .values()
            .filter(|action| action.journey_id == journey_id)
            .cloned()
            .collect();
        actions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(JourneyDetailDto { journey, actions })
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::JourneyNotFound(journey_id.to_string()));
        }
        let journey = state
            .journeys
            .iter_mut()
            .find(|journey| journey.id == journey_id)
            .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        if let Some(title) = request.title {
            journey.title = title.trim().to_string();
        }
        if let Some(intent) = request.intent {
            journey.intent = intent.trim().to_string();
        }
        if let Some(duration_label) = request.duration_label {
            journey.duration_label = duration_label.trim().to_string();
        }
        if let Some(status) = request.status {
            journey.status = status;
            if status == JourneyStatusDto::Completed {
                journey.progress = 100;
            }
        }
        Ok(journey.clone())
    }

    async fn create_action(
        &self,
        user_id: &str,
        action: ActionDto,
    ) -> Result<ActionDto, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .journey_owners
            .get(&action.journey_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::JourneyNotFound(action.journey_id));
        }
        state
            .action_owners
            .insert(action.id.clone(), user_id.to_string());
        state.actions.insert(action.id.clone(), action.clone());
        Ok(action)
    }

    async fn today(&self, user_id: &str) -> Result<Vec<ActionDto>, RepositoryError> {
        let state = self.state.read().await;
        let mut actions: Vec<_> = state
            .actions
            .values()
            .filter(|action| {
                state
                    .action_owners
                    .get(&action.id)
                    .is_some_and(|owner| owner == user_id)
            })
            .cloned()
            .collect();
        actions.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(actions)
    }

    async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::ActionNotFound(action_id.to_string()));
        }
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        action.state = ActionStateDto::Completed;
        let updated = action.clone();
        let journey_id = updated.journey_id.clone();
        let total = state
            .actions
            .values()
            .filter(|item| item.journey_id == journey_id)
            .count();
        let completed = state
            .actions
            .values()
            .filter(|item| item.journey_id == journey_id && item.state == ActionStateDto::Completed)
            .count();
        let next_action = state
            .actions
            .values()
            .find(|item| item.journey_id == journey_id && item.state == ActionStateDto::Pending)
            .map(|item| item.title.clone())
            .unwrap_or_else(|| "路线已完成".to_string());
        if let Some(journey) = state.journeys.iter_mut().find(|item| item.id == journey_id) {
            journey.progress = if total == 0 {
                0
            } else {
                ((completed * 100) / total) as u8
            };
            journey.next_action = next_action;
        }
        Ok(updated)
    }

    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, RepositoryError> {
        let mut state = self.state.write().await;
        if !state
            .action_owners
            .get(action_id)
            .is_some_and(|owner| owner == user_id)
        {
            return Err(RepositoryError::ActionNotFound(action_id.to_string()));
        }
        let action = state
            .actions
            .get_mut(action_id)
            .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        if let Some(title) = request.title {
            action.title = title.trim().to_string();
        }
        if let Some(detail) = request.detail {
            action.detail = detail.trim().to_string();
        }
        if let Some(estimated_minutes) = request.estimated_minutes {
            action.estimated_minutes = estimated_minutes;
        }
        if let Some(scheduled_label) = request.scheduled_label {
            action.scheduled_label = scheduled_label.trim().to_string();
        }
        if let Some(state) = request.state {
            action.state = state;
        }
        let updated = action.clone();
        let journey_id = updated.journey_id.clone();
        let total = state
            .actions
            .values()
            .filter(|item| item.journey_id == journey_id)
            .count();
        let completed = state
            .actions
            .values()
            .filter(|item| item.journey_id == journey_id && item.state == ActionStateDto::Completed)
            .count();
        if let Some(journey) = state.journeys.iter_mut().find(|item| item.id == journey_id) {
            journey.progress = if total == 0 {
                0
            } else {
                ((completed * 100) / total) as u8
            };
        }
        Ok(updated)
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

    async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, RepositoryError> {
        let journey = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        let journey = serde_json::from_value(journey).map_err(RepositoryError::Serialization)?;
        let actions = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE journey_id = $1 AND user_id = $2 ORDER BY created_at",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .into_iter()
        .map(|payload| serde_json::from_value(payload).map_err(RepositoryError::Serialization))
        .collect::<Result<Vec<ActionDto>, _>>()?;
        Ok(JourneyDetailDto { journey, actions })
    }

    async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, RepositoryError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM journeys WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(journey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::JourneyNotFound(journey_id.to_string()))?;
        let mut journey: JourneyDto =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        if let Some(title) = request.title {
            journey.title = title.trim().to_string();
        }
        if let Some(intent) = request.intent {
            journey.intent = intent.trim().to_string();
        }
        if let Some(duration_label) = request.duration_label {
            journey.duration_label = duration_label.trim().to_string();
        }
        if let Some(status) = request.status {
            journey.status = status;
            if status == JourneyStatusDto::Completed {
                journey.progress = 100;
            }
        }
        sqlx::query(
            "UPDATE journeys SET payload = $1, status = $2, progress = $3, updated_at = now() WHERE id = $4 AND user_id = $5",
        )
        .bind(serde_json::to_value(&journey).map_err(RepositoryError::Serialization)?)
        .bind(format_status(journey.status))
        .bind(i32::from(journey.progress))
        .bind(journey_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(journey)
    }

    async fn create_action(
        &self,
        user_id: &str,
        action: ActionDto,
    ) -> Result<ActionDto, RepositoryError> {
        let journey_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM journeys WHERE id = $1 AND user_id = $2)",
        )
        .bind(&action.journey_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        if !journey_exists {
            return Err(RepositoryError::JourneyNotFound(action.journey_id));
        }
        sqlx::query(
            "INSERT INTO actions (id, journey_id, user_id, payload, state) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&action.id)
        .bind(&action.journey_id)
        .bind(user_id)
        .bind(serde_json::to_value(&action).map_err(RepositoryError::Serialization)?)
        .bind(format_action_state(action.state))
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(action)
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
        sqlx::query(
            "WITH progress AS (SELECT journey_id, COALESCE(((COUNT(*) FILTER (WHERE state = 'completed') * 100) / NULLIF(COUNT(*), 0))::int, 0) AS value FROM actions WHERE user_id = $2 AND journey_id = (SELECT journey_id FROM actions WHERE id = $1 AND user_id = $2) GROUP BY journey_id) UPDATE journeys AS j SET progress = progress.value, payload = jsonb_set(j.payload, '{progress}', to_jsonb(progress.value), true), updated_at = now() FROM progress WHERE j.id = progress.journey_id AND j.user_id = $2",
        )
        .bind(action_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        serde_json::from_value(payload).map_err(RepositoryError::Serialization)
    }

    async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, RepositoryError> {
        let payload = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT payload FROM actions WHERE id = $1 AND user_id = $2 FOR UPDATE",
        )
        .bind(action_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepositoryError::Database)?
        .ok_or_else(|| RepositoryError::ActionNotFound(action_id.to_string()))?;
        let mut action: ActionDto =
            serde_json::from_value(payload).map_err(RepositoryError::Serialization)?;
        if let Some(title) = request.title {
            action.title = title.trim().to_string();
        }
        if let Some(detail) = request.detail {
            action.detail = detail.trim().to_string();
        }
        if let Some(estimated_minutes) = request.estimated_minutes {
            action.estimated_minutes = estimated_minutes;
        }
        if let Some(scheduled_label) = request.scheduled_label {
            action.scheduled_label = scheduled_label.trim().to_string();
        }
        if let Some(state) = request.state {
            action.state = state;
        }
        sqlx::query(
            "UPDATE actions SET payload = $1, state = $2, updated_at = now() WHERE id = $3 AND user_id = $4",
        )
        .bind(serde_json::to_value(&action).map_err(RepositoryError::Serialization)?)
        .bind(format_action_state(action.state))
        .bind(action_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        sqlx::query(
            "WITH progress AS (SELECT journey_id, COALESCE(((COUNT(*) FILTER (WHERE state = 'completed') * 100) / NULLIF(COUNT(*), 0))::int, 0) AS value FROM actions WHERE user_id = $2 AND journey_id = (SELECT journey_id FROM actions WHERE id = $1 AND user_id = $2) GROUP BY journey_id) UPDATE journeys AS j SET progress = progress.value, payload = jsonb_set(j.payload, '{progress}', to_jsonb(progress.value), true), updated_at = now() FROM progress WHERE j.id = progress.journey_id AND j.user_id = $2",
        )
        .bind(action_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(RepositoryError::Database)?;
        Ok(action)
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
