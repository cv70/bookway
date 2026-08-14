use uuid::Uuid;

use crate::api::{
    ActionDto, ActionStateDto, CreateActionRequest, CreateJourneyRequest, JourneyDetailDto,
    JourneyDto, JourneyStatusDto, TodayDto, UpdateActionRequest, UpdateJourneyRequest,
};
use crate::domain::{Domain, GrowthError};

impl Domain {
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

    pub(crate) async fn get_journey(
        &self,
        user_id: &str,
        journey_id: &str,
    ) -> Result<JourneyDetailDto, GrowthError> {
        Ok(self.repository.get_journey(user_id, journey_id).await?)
    }

    pub(crate) async fn update_journey(
        &self,
        user_id: &str,
        journey_id: &str,
        request: UpdateJourneyRequest,
    ) -> Result<JourneyDto, GrowthError> {
        validate_journey_update(&request)?;
        Ok(self
            .repository
            .update_journey(user_id, journey_id, request)
            .await?)
    }

    pub(crate) async fn create_action(
        &self,
        user_id: &str,
        request: CreateActionRequest,
    ) -> Result<ActionDto, GrowthError> {
        validate_action(
            &request.title,
            request.estimated_minutes,
            &request.scheduled_label,
        )?;
        let action = ActionDto {
            id: Uuid::now_v7().to_string(),
            journey_id: request.journey_id,
            title: request.title.trim().to_string(),
            detail: request.detail.trim().to_string(),
            estimated_minutes: request.estimated_minutes,
            scheduled_label: request.scheduled_label.trim().to_string(),
            state: ActionStateDto::Pending,
        };
        Ok(self.repository.create_action(user_id, action).await?)
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

    pub(crate) async fn update_action(
        &self,
        user_id: &str,
        action_id: &str,
        request: UpdateActionRequest,
    ) -> Result<ActionDto, GrowthError> {
        if let Some(title) = &request.title {
            if title.trim().is_empty() {
                return Err(GrowthError::Validation("行动名称不能为空".to_string()));
            }
        }
        if let Some(minutes) = request.estimated_minutes {
            if minutes == 0 || minutes > 720 {
                return Err(GrowthError::Validation(
                    "行动时长需要在 1 到 720 分钟之间".to_string(),
                ));
            }
        }
        if request
            .scheduled_label
            .as_deref()
            .is_some_and(|label| label.trim().is_empty())
        {
            return Err(GrowthError::Validation("安排时间不能为空".to_string()));
        }
        Ok(self
            .repository
            .update_action(user_id, action_id, request)
            .await?)
    }
}

fn validate_journey_update(request: &UpdateJourneyRequest) -> Result<(), GrowthError> {
    if request
        .title
        .as_deref()
        .is_some_and(|title| title.trim().is_empty())
    {
        return Err(GrowthError::Validation("路线名称不能为空".to_string()));
    }
    if request
        .duration_label
        .as_deref()
        .is_some_and(|duration| duration.trim().is_empty())
    {
        return Err(GrowthError::Validation("路线周期不能为空".to_string()));
    }
    Ok(())
}

fn validate_action(
    title: &str,
    estimated_minutes: u16,
    scheduled_label: &str,
) -> Result<(), GrowthError> {
    if title.trim().is_empty() {
        return Err(GrowthError::Validation("行动名称不能为空".to_string()));
    }
    if estimated_minutes == 0 || estimated_minutes > 720 {
        return Err(GrowthError::Validation(
            "行动时长需要在 1 到 720 分钟之间".to_string(),
        ));
    }
    if scheduled_label.trim().is_empty() {
        return Err(GrowthError::Validation("安排时间不能为空".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api::GrowthDomainDto;
    use crate::{conf::Config, datasource::MemoryGrowthRepository};

    fn domain() -> Domain {
        Domain::from_repository(
            Config {
                listen_addr: "127.0.0.1:0".parse().unwrap(),
            },
            Arc::new(MemoryGrowthRepository::seeded()),
        )
    }

    #[tokio::test]
    async fn creates_a_journey_with_its_first_action() {
        let domain = domain();
        let journey = domain
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
            domain
                .today("user-a")
                .await
                .expect("today should load")
                .total,
            1
        );
    }

    #[tokio::test]
    async fn memory_storage_keeps_journeys_and_actions_isolated_by_user() {
        let domain = domain();

        assert!(
            domain
                .list_journeys("another-user")
                .await
                .expect("journeys should load")
                .is_empty()
        );
        assert!(
            domain
                .today("another-user")
                .await
                .expect("today should load")
                .actions
                .is_empty()
        );
        assert!(
            domain
                .complete_action("another-user", "action-stretch")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rejects_an_empty_title() {
        let domain = domain();
        let result = domain
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

    #[tokio::test]
    async fn manages_route_and_action_lifecycle() {
        let domain = domain();
        let journey = domain
            .create_journey(
                "user-a",
                CreateJourneyRequest {
                    title: "四周写作练习".to_string(),
                    intent: "留下可回看的作品".to_string(),
                    domain: GrowthDomainDto::Learning,
                    duration_label: "4 周".to_string(),
                    first_action_title: "写 100 字".to_string(),
                    first_action_detail: String::new(),
                    estimated_minutes: 10,
                },
            )
            .await
            .expect("journey should be created");

        let detail = domain
            .get_journey("user-a", &journey.id)
            .await
            .expect("journey detail should load");
        assert_eq!(detail.actions.len(), 1);

        let action = domain
            .create_action(
                "user-a",
                CreateActionRequest {
                    journey_id: journey.id.clone(),
                    title: "修改开头".to_string(),
                    detail: "让第一段更具体".to_string(),
                    estimated_minutes: 15,
                    scheduled_label: "明天".to_string(),
                },
            )
            .await
            .expect("action should be created");
        let updated = domain
            .update_action(
                "user-a",
                &action.id,
                UpdateActionRequest {
                    state: Some(ActionStateDto::Skipped),
                    ..Default::default()
                },
            )
            .await
            .expect("action should update");
        assert_eq!(updated.state, ActionStateDto::Skipped);

        let paused = domain
            .update_journey(
                "user-a",
                &journey.id,
                UpdateJourneyRequest {
                    status: Some(JourneyStatusDto::Paused),
                    ..Default::default()
                },
            )
            .await
            .expect("journey should update");
        assert_eq!(paused.status, JourneyStatusDto::Paused);
    }
}
