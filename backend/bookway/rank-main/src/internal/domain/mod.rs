use std::sync::Arc;

use super::{
    api::{RankRequest, RankedItem},
    datasource::HeuristicRanker,
};

#[derive(Clone)]
pub(crate) struct RankService {
    ranker: Arc<HeuristicRanker>,
}
impl RankService {
    pub(crate) fn new(ranker: Arc<HeuristicRanker>) -> Self {
        Self { ranker }
    }
    pub(crate) fn rank(&self, request: RankRequest) -> Vec<RankedItem> {
        self.ranker.rank(request)
    }
}
