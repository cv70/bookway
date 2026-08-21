#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, mall_order_server::MallOrder};
use crate::{Domain, domain::OrderError};
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}
#[tonic::async_trait]
impl MallOrder for GrpcServer {
    async fn create(
        &self,
        request: Request<pb::CreateRequest>,
    ) -> Result<Response<pb::Order>, Status> {
        Ok(Response::new(
            self.domain
                .create(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn list(
        &self,
        request: Request<pb::UserRequest>,
    ) -> Result<Response<pb::OrderListResponse>, Status> {
        Ok(Response::new(
            self.domain
                .list(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn get(&self, request: Request<pb::OrderRequest>) -> Result<Response<pb::Order>, Status> {
        Ok(Response::new(
            self.domain
                .get(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn pay(&self, request: Request<pb::PayRequest>) -> Result<Response<pb::Order>, Status> {
        Ok(Response::new(
            self.domain
                .pay(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn cancel(
        &self,
        request: Request<pb::OrderRequest>,
    ) -> Result<Response<pb::Order>, Status> {
        Ok(Response::new(
            self.domain
                .cancel(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn expire_pending(
        &self,
        request: Request<pb::BatchRequest>,
    ) -> Result<Response<pb::ExpirePendingResponse>, Status> {
        Ok(Response::new(
            self.domain
                .expire_pending(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn merchant_orders(
        &self,
        request: Request<pb::MerchantOrderRequest>,
    ) -> Result<Response<pb::MerchantOrderListResponse>, Status> {
        Ok(Response::new(
            self.domain
                .merchant_orders(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn update_fulfillment(
        &self,
        request: Request<pb::UpdateFulfillmentRequest>,
    ) -> Result<Response<pb::Order>, Status> {
        Ok(Response::new(
            self.domain
                .update_fulfillment(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn affiliate_settlements(
        &self,
        request: Request<pb::AffiliateSettlementRequest>,
    ) -> Result<Response<pb::AffiliateSettlementListResponse>, Status> {
        Ok(Response::new(
            self.domain
                .affiliate_settlements(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
    async fn settle_affiliate(
        &self,
        request: Request<pb::SettleAffiliateRequest>,
    ) -> Result<Response<pb::AffiliateSettlement>, Status> {
        Ok(Response::new(
            self.domain
                .settle_affiliate(request.into_inner())
                .await
                .map_err(order_error)?,
        ))
    }
}
pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::mall_order_server::MallOrderServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::mall_order_server::MallOrderServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}
fn order_error(error: OrderError) -> Status {
    match error {
        OrderError::Validation(message) => Status::invalid_argument(message),
        OrderError::NotFound(message) => Status::not_found(message),
        OrderError::Conflict(message) => Status::already_exists(message),
        OrderError::State(message) => Status::failed_precondition(message),
        OrderError::Upstream(_, message) => Status::unavailable(message),
        OrderError::Dao(message) => Status::internal(message),
    }
}
