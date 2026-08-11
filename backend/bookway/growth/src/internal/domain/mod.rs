use std::sync::Arc;

use super::api::{
    ActionDto, ActionStateDto, CreateJourneyRequest, JourneyDto, JourneyStatusDto, TodayDto,
};
use thiserror::Error;
use uuid::Uuid;

use super::datasource::{GrowthRepository, RepositoryError};

#[derive(Debug, Error)]
pub(crate) enum GrowthError {
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

#[derive(Clone)]
pub(crate) struct GrowthService {
    repository: Arc<dyn GrowthRepository>,
}

impl GrowthService {
    pub(crate) fn new(repository: Arc<dyn GrowthRepository>) -> Self {
        Self { repository }
    }

    pub(crate) async fn list_journeys(
        &self,
        user_id: &str,
    ) -> Result<Vec<JourneyDto>, GrowthError> {
        Ok(self.repository.list_journeys(user_id).await?)
    }

    pub(crate) async fn create_journey(
        &self,
        user_id: &str,
        request: CreateJourneyRequest,
    ) -> Result<JourneyDto, GrowthError> {
        if request.title.trim().is_empty() {
            return Err(GrowthError::Validation("路线名称不能为空".to_string()));
        }
        if request.first_action_title.trim().is_empty() {
            return Err(GrowthError::Validation("第一个行动不能为空".to_string()));
        }
        if request.estimated_minutes == 0 || request.estimated_minutes > 720 {
            return Err(GrowthError::Validation(
                "行动时长需要在 1 到 720 分钟之间".to_string(),
            ));
        }

        let journey_id = Uuid::now_v7().to_string();
        let journey = JourneyDto {
            id: journey_id.clone(),
            title: request.title.trim().to_string(),
            intent: request.intent.trim().to_string(),
            domain: request.domain,
            status: JourneyStatusDto::Active,
            progress: 0,
            duration_label: request.duration_label,
            next_action: request.first_action_title.trim().to_string(),
            participant_count: 1,
        };
        let first_action = ActionDto {
            id: Uuid::now_v7().to_string(),
            journey_id,
            title: request.first_action_title.trim().to_string(),
            detail: request.first_action_detail.trim().to_string(),
            estimated_minutes: request.estimated_minutes,
            scheduled_label: "今天".to_string(),
            state: ActionStateDto::Pending,
        };

        Ok(self
            .repository
            .create_journey(user_id, journey, first_action)
            .await?)
    }

    pub(crate) async fn today(&self, user_id: &str) -> Result<TodayDto, GrowthError> {
        let actions = self.repository.today(user_id).await?;
        let completed = actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .count();
        let focus_minutes = actions
            .iter()
            .filter(|action| action.state == ActionStateDto::Completed)
            .map(|action| u32::from(action.estimated_minutes))
            .sum();
        Ok(TodayDto {
            completed,
            total: actions.len(),
            focus_minutes,
            actions,
        })
    }

    pub(crate) async fn complete_action(
        &self,
        user_id: &str,
        action_id: &str,
    ) -> Result<ActionDto, GrowthError> {
        Ok(self.repository.complete_action(user_id, action_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::api::GrowthDomainDto;
    use crate::internal::datasource::MemoryGrowthRepository;

    #[tokio::test]
    async fn creates_a_journey_with_its_first_action() {
        let service = GrowthService::new(Arc::new(MemoryGrowthRepository::seeded()));
        let journey = service
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: "学习摄影".to_string(),
                    intent: "记录旅行".to_string(),
                    domain: GrowthDomainDto::Learning,
                    duration_label: "3 周".to_string(),
                    first_action_title: "理解曝光三要素".to_string(),
                    first_action_detail: "完成一组对比照片".to_string(),
                    estimated_minutes: 25,
                },
            )
            .await
            .expect("journey should be created");

        assert_eq!(journey.title, "学习摄影");
        assert_eq!(
            service
                .today("user-a")
                .await
                .expect("today should load")
                .total,
            4
        );
    }

    #[tokio::test]
    async fn rejects_an_empty_title() {
        let service = GrowthService::new(Arc::new(MemoryGrowthRepository::seeded()));
        let result = service
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: " ".to_string(),
                    intent: String::new(),
                    domain: GrowthDomainDto::Learning,
                    duration_label: "1 周".to_string(),
                    first_action_title: "开始".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 10,
                },
            )
            .await;

        assert!(matches!(result, Err(GrowthError::Validation(_))));
    }
}
