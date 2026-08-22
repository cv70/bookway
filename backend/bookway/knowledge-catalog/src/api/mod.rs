use crate::domain::{Domain, DomainError};
use bookway_knowledge_catalog_api::pb::{
    self,
    knowledge_catalog_server::{KnowledgeCatalog, KnowledgeCatalogServer},
};
use tonic::{Request, Response, Status};

#[derive(Clone)]
struct GrpcServer {
    domain: Domain,
}

#[tonic::async_trait]
impl KnowledgeCatalog for GrpcServer {
    async fn search(
        &self,
        request: Request<pb::SearchRequest>,
    ) -> Result<Response<pb::SearchResponse>, Status> {
        Ok(Response::new(
            self.domain
                .search(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }
    async fn get(
        &self,
        request: Request<pb::GetRequest>,
    ) -> Result<Response<pb::Resource>, Status> {
        Ok(Response::new(
            self.domain
                .get(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn list_node_resources(
        &self,
        request: Request<pb::ListNodeResourcesRequest>,
    ) -> Result<Response<pb::ListNodeResourcesResponse>, Status> {
        Ok(Response::new(
            self.domain
                .list_node_resources(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn attach_node_resource(
        &self,
        request: Request<pb::AttachNodeResourceRequest>,
    ) -> Result<Response<pb::RouteNodeResourceAttachment>, Status> {
        Ok(Response::new(
            self.domain
                .attach_node_resource(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn detach_node_resource(
        &self,
        request: Request<pb::DetachNodeResourceRequest>,
    ) -> Result<Response<pb::DetachNodeResourceResponse>, Status> {
        Ok(Response::new(
            self.domain
                .detach_node_resource(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn retrieve_rag_context(
        &self,
        request: Request<pb::RetrieveRagContextRequest>,
    ) -> Result<Response<pb::RetrieveRagContextResponse>, Status> {
        Ok(Response::new(
            self.domain
                .retrieve_rag_context(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn upsert_rag_embedding(
        &self,
        request: Request<pb::UpsertRagEmbeddingRequest>,
    ) -> Result<Response<pb::UpsertRagEmbeddingResponse>, Status> {
        Ok(Response::new(
            self.domain
                .upsert_rag_embedding(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }

    async fn search_rag_embeddings(
        &self,
        request: Request<pb::SearchRagEmbeddingsRequest>,
    ) -> Result<Response<pb::SearchRagEmbeddingsResponse>, Status> {
        Ok(Response::new(
            self.domain
                .search_rag_embeddings(request.into_inner())
                .await
                .map_err(status)?,
        ))
    }
}

pub(crate) async fn serve(domain: Domain) -> Result<(), tonic::transport::Error> {
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<KnowledgeCatalogServer<GrpcServer>>()
        .await;
    tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(KnowledgeCatalogServer::with_interceptor(
            GrpcServer {
                domain: domain.clone(),
            },
            bookway_runtime::grpc_service_auth_interceptor,
        ))
        .serve(domain.config().listen_addr)
        .await
}

fn status(error: DomainError) -> Status {
    match error {
        DomainError::Validation(message) => Status::invalid_argument(message),
        DomainError::Repository(crate::datasource::DaoError::NotFound(message)) => {
            Status::not_found(message)
        }
        DomainError::Repository(crate::datasource::DaoError::Conflict(message)) => {
            Status::already_exists(message)
        }
        DomainError::Repository(error) => Status::internal(error.to_string()),
        DomainError::Upstream(message) => Status::internal(message),
    }
}
