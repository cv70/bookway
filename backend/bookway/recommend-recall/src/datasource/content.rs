use bookway_api::{ContentPageDto, ContentQueryRequest, ContentStatusDto, GrowthDomainDto};
use bookway_bbs_link::api::pb::{self, bbs_link_client::BbsLinkClient};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum DataSourceError {
    #[error("bbs-link request failed: {0}")]
    Request(String),
}

#[derive(Clone)]
pub(crate) struct ContentDataSource {
    client: BbsLinkClient<tonic::transport::Channel>,
}

impl ContentDataSource {
    pub(crate) async fn connect(base_url: String) -> Result<Self, tonic::transport::Error> {
        Ok(Self {
            client: BbsLinkClient::connect(base_url).await?,
        })
    }

    pub(crate) async fn list(
        &self,
        strategy: &str,
        cursor: Option<String>,
        limit: usize,
        domain: Option<GrowthDomainDto>,
    ) -> Result<ContentPageDto, DataSourceError> {
        let request = ContentQueryRequest {
            cursor,
            limit: Some(limit),
            status: Some(ContentStatusDto::Published),
            strategy: Some(strategy.to_string()),
            ids: None,
            content_type: None,
            domain,
        };
        let mut client = self.client.clone();
        let response = client
            .list(pb::ListRequest {
                request_json: serde_json::to_string(&request)
                    .map_err(|error| DataSourceError::Request(error.to_string()))?,
            })
            .await
            .map_err(|error| DataSourceError::Request(error.to_string()))?
            .into_inner();
        serde_json::from_str(&response.response_json)
            .map_err(|error| DataSourceError::Request(error.to_string()))
    }
}
