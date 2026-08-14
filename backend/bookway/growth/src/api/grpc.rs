use super::pb::{self, growth_server::Growth};
use crate::{
    api::{CreateActionRequest, CreateJourneyRequest, UpdateActionRequest, UpdateJourneyRequest},
    domain::Domain,
};
use serde::Serialize;
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl Growth for GrpcServer {
    async fn list_journeys(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(
            &self
                .domain
                .list_journeys(&user_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_journey(
        &self,
        request: Request<pb::CreateJourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateJourneyRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_journey(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn get_journey(
        &self,
        request: Request<pb::JourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .get_journey(&request.user_id, &request.journey_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_journey(
        &self,
        request: Request<pb::UpdateJourneyRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateJourneyRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_journey(&request.user_id, &request.journey_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn create_action(
        &self,
        request: Request<pb::CreateActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: CreateActionRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .create_action(&request.user_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn today(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let user_id = request.into_inner().user_id;
        json_response(&self.domain.today(&user_id).await.map_err(internal_error)?)
    }

    async fn complete_action(
        &self,
        request: Request<pb::CompleteActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        json_response(
            &self
                .domain
                .complete_action(&request.user_id, &request.action_id)
                .await
                .map_err(internal_error)?,
        )
    }

    async fn update_action(
        &self,
        request: Request<pb::UpdateActionRequest>,
    ) -> Result<Response<pb::JsonResponse>, Status> {
        let request = request.into_inner();
        let payload: UpdateActionRequest = from_json(&request.request_json)?;
        json_response(
            &self
                .domain
                .update_action(&request.user_id, &request.action_id, payload)
                .await
                .map_err(internal_error)?,
        )
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let addr = domain.config.listen_addr;
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::growth_server::GrowthServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::growth_server::GrowthServer::new(GrpcServer { domain }))
        .serve(addr)
        .await
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal_error(error: crate::domain::GrowthError) -> Status {
    Status::internal(error.to_string())
}

fn json_response<T: Serialize>(value: &T) -> Result<Response<pb::JsonResponse>, Status> {
    Ok(Response::new(pb::JsonResponse {
        response_json: serde_json::to_string(value)
            .map_err(|error| Status::internal(error.to_string()))?,
    }))
}
