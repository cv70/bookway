use std::sync::Arc;

use crate::{conf::Config, datasource::ContentDataSource};

#[derive(Clone)]
pub struct Domain {
    pub(crate) config: Config,
    pub(crate) content: Arc<ContentDataSource>,
    pub(crate) max_candidates: usize,
}

impl Domain {
    pub async fn new(config: Config) -> Result<Self, tonic::transport::Error> {
        let content = ContentDataSource::connect(config.bbs_link_url.clone()).await?;
        Ok(Self {
            content: Arc::new(content),
            max_candidates: config.max_candidates,
            config,
        })
    }
}
