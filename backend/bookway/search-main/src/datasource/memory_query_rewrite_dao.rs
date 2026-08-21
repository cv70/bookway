use super::*;

pub(crate) struct MemoryQueryRewriteDao;

#[async_trait]
impl QueryRewriteDao for MemoryQueryRewriteDao {
    async fn active(&self) -> Result<Option<QueryRewriteDictionary>, QueryRewriteError> {
        Ok(Some(builtin_query_rewrite_dictionary()))
    }
}
