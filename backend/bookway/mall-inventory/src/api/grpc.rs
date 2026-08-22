#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, mall_inventory_server::MallInventory};
use crate::{Domain, domain::InventoryError};
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}
#[tonic::async_trait]
impl MallInventory for GrpcServer {
    async fn set_stock(
        &self,
        request: Request<pb::SetStockRequest>,
    ) -> Result<Response<pb::InventoryItem>, Status> {
        Ok(Response::new(
            self.domain
                .set_stock(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
    async fn stock(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::InventoryItem>, Status> {
        Ok(Response::new(
            self.domain
                .stock(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
    async fn reserve(
        &self,
        request: Request<pb::ReserveRequest>,
    ) -> Result<Response<pb::Reservation>, Status> {
        Ok(Response::new(
            self.domain
                .reserve(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
    async fn confirm(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::Reservation>, Status> {
        Ok(Response::new(
            self.domain
                .confirm(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
    async fn release(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::Reservation>, Status> {
        Ok(Response::new(
            self.domain
                .release(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
    async fn expire_reservations(
        &self,
        request: Request<pb::BatchRequest>,
    ) -> Result<Response<pb::ExpireReservationsResponse>, Status> {
        Ok(Response::new(
            self.domain
                .expire_reservations(request.into_inner())
                .await
                .map_err(inventory_error)?,
        ))
    }
}
pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::mall_inventory_server::MallInventoryServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(
            pb::mall_inventory_server::MallInventoryServer::with_interceptor(
                GrpcServer {
                    domain: domain.clone(),
                },
                bookway_runtime::grpc_service_auth_interceptor,
            ),
        )
        .serve(domain.config().listen_addr)
        .await
}
fn inventory_error(error: InventoryError) -> Status {
    match error {
        InventoryError::Validation(message) => Status::invalid_argument(message),
        InventoryError::NotFound(message) => Status::not_found(message),
        InventoryError::Insufficient(message) => Status::failed_precondition(message),
        InventoryError::Conflict(message) => Status::already_exists(message),
        InventoryError::Repository(message) => Status::internal(message),
    }
}
