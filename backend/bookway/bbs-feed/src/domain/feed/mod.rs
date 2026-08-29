use bookway_recommend_main_api::pb;

use crate::domain::Domain;

impl Domain {
    pub(crate) async fn feed(
        &self,
        mut request: pb::FeedRequest,
    ) -> Result<pb::FeedResponse, tonic::Status> {
        request.limit = Some(request.limit.unwrap_or(10).clamp(1, 20));
        if request.surface.trim().is_empty() {
            request.surface = "home".to_string();
        }
        let mut client = self.recommend_main.clone();
        let request = bookway_runtime::grpc_service_request(request)
            .map_err(|error| tonic::Status::unauthenticated(error.to_string()))?;
        Ok(self
            .recommend_breaker
            .execute(client.feed(request))
            .await?
            .into_inner())
    }
}
