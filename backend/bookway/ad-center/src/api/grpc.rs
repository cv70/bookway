#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, ad_center_server::AdCenter};
use crate::{Domain, domain::AdCenterError};
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl AdCenter for GrpcServer {
    async fn create_campaign(
        &self,
        request: Request<pb::CreateCampaignRequest>,
    ) -> Result<Response<pb::AdCampaign>, Status> {
        Ok(Response::new(
            self.domain
                .create_campaign(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn update_campaign(
        &self,
        request: Request<pb::UpdateCampaignRequest>,
    ) -> Result<Response<pb::AdCampaign>, Status> {
        Ok(Response::new(
            self.domain
                .update_campaign(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn campaign(
        &self,
        request: Request<pb::CampaignIdRequest>,
    ) -> Result<Response<pb::AdCampaign>, Status> {
        Ok(Response::new(
            self.domain
                .campaign(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn campaigns(
        &self,
        request: Request<pb::AdvertiserCampaignQuery>,
    ) -> Result<Response<pb::CampaignList>, Status> {
        Ok(Response::new(
            self.domain
                .campaigns(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn eligible(
        &self,
        request: Request<pb::EligibleRequest>,
    ) -> Result<Response<pb::CampaignList>, Status> {
        Ok(Response::new(
            self.domain
                .eligible(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn record_event(
        &self,
        request: Request<pb::RecordEventRequest>,
    ) -> Result<Response<pb::EventReceipt>, Status> {
        Ok(Response::new(
            self.domain
                .record_event(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn register_decisions(
        &self,
        request: Request<pb::RegisterDecisionRequest>,
    ) -> Result<Response<pb::EmptyResponse>, Status> {
        self.domain
            .register_decisions(request.into_inner())
            .await
            .map_err(ad_error)?;
        Ok(Response::new(pb::EmptyResponse {}))
    }

    async fn get_delivery_guardrails(
        &self,
        _request: Request<pb::GetDeliveryGuardrailsRequest>,
    ) -> Result<Response<pb::DeliveryGuardrails>, Status> {
        Ok(Response::new(
            self.domain.delivery_guardrails().await.map_err(ad_error)?,
        ))
    }

    async fn set_user_daily_total_cap(
        &self,
        request: Request<pb::DeliveryGuardrails>,
    ) -> Result<Response<pb::DeliveryGuardrails>, Status> {
        Ok(Response::new(
            self.domain
                .set_user_daily_total_cap(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }

    async fn delivery_report(
        &self,
        request: Request<pb::AdDeliveryReportRequest>,
    ) -> Result<Response<pb::AdDeliveryReport>, Status> {
        Ok(Response::new(
            self.domain
                .advertiser_delivery_report(request.into_inner())
                .await
                .map_err(ad_error)?,
        ))
    }
}

pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::ad_center_server::AdCenterServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::ad_center_server::AdCenterServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}

fn ad_error(error: AdCenterError) -> Status {
    match error {
        AdCenterError::Validation(message) => Status::invalid_argument(message),
        AdCenterError::NotFound(message) => Status::not_found(message),
        AdCenterError::Repository(message) => Status::internal(message),
    }
}
