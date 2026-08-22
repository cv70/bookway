#![allow(clippy::result_large_err)] // tonic::Status is fixed by the transport API.

use super::pb::{self, mall_server::Mall};
use crate::{Domain, domain::MallError};
use tonic::{Request, Response, Status};
#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}
#[tonic::async_trait]
impl Mall for GrpcServer {
    async fn create_product(
        &self,
        request: Request<pb::CreateProductRequest>,
    ) -> Result<Response<pb::MallProduct>, Status> {
        Ok(Response::new(
            self.domain
                .create_product(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn update_product(
        &self,
        request: Request<pb::UpdateProductRequest>,
    ) -> Result<Response<pb::MallProduct>, Status> {
        Ok(Response::new(
            self.domain
                .update_product(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn products(
        &self,
        request: Request<pb::ProductQueryRequest>,
    ) -> Result<Response<pb::ProductPage>, Status> {
        Ok(Response::new(
            self.domain
                .products(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn product(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::MallProduct>, Status> {
        Ok(Response::new(
            self.domain
                .product(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn skus(
        &self,
        request: Request<pb::SkuIdsRequest>,
    ) -> Result<Response<pb::SkuListResponse>, Status> {
        Ok(Response::new(
            self.domain
                .skus(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn attach_node_offer(
        &self,
        request: Request<pb::AttachNodeOfferRequest>,
    ) -> Result<Response<pb::NodeOffer>, Status> {
        Ok(Response::new(
            self.domain
                .attach_node_offer(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn node_offers(
        &self,
        request: Request<pb::NodeOfferQueryRequest>,
    ) -> Result<Response<pb::NodeOfferList>, Status> {
        Ok(Response::new(
            self.domain
                .node_offers(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn get_node_offer(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::NodeOffer>, Status> {
        Ok(Response::new(
            self.domain
                .settlement_node_offer(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
    async fn get_checkout_node_offer(
        &self,
        request: Request<pb::IdRequest>,
    ) -> Result<Response<pb::NodeOffer>, Status> {
        Ok(Response::new(
            self.domain
                .checkout_node_offer(request.into_inner())
                .await
                .map_err(mall_error)?,
        ))
    }
}
pub async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<pb::mall_server::MallServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(pb::mall_server::MallServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}
fn mall_error(error: MallError) -> Status {
    match error {
        MallError::Validation(message) => Status::invalid_argument(message),
        MallError::NotFound(message) => Status::not_found(message),
        MallError::Conflict(message) => Status::already_exists(message),
        MallError::Repository(message) => Status::internal(message),
    }
}
