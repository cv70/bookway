use std::sync::Arc;

use thiserror::Error;

use super::{
    api::{SearchQueryRequest, SearchResponseDto, SuggestionResponseDto},
    datasource::{SearchClientError, SearchDataSource},
};

const MAX_QUERY_LENGTH: usize = 200;

#[derive(Debug, Error)]
pub(crate) enum SearchMainError {
    #[error("search query must not be empty")]
    EmptyQuery,
    #[error("search query exceeds {MAX_QUERY_LENGTH} characters")]
    QueryTooLong,
    #[error(transparent)]
    Upstream(#[from] SearchClientError),
}

#[derive(Clone)]
pub(crate) struct SearchMainService {
    search: Arc<dyn SearchDataSource>,
}

impl SearchMainService {
    pub(crate) fn new(search: Arc<dyn SearchDataSource>) -> Self {
        Self { search }
    }

    pub(crate) async fn search(
        &self,
        mut request: SearchQueryRequest,
    ) -> Result<SearchResponseDto, SearchMainError> {
        request.q = normalize_query(&request.q)?;
        request.limit = Some(request.limit.unwrap_or(20).clamp(1, 50));
        Ok(self.search.search(request).await?)
    }

    pub(crate) async fn suggestions(
        &self,
        query: &str,
    ) -> Result<SuggestionResponseDto, SearchMainError> {
        let query = normalize_query(query)?;
        Ok(self.search.suggestions(&query).await?)
    }
}

fn normalize_query(query: &str) -> Result<String, SearchMainError> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() {
        return Err(SearchMainError::EmptyQuery);
    }
    if query.chars().count() > MAX_QUERY_LENGTH {
        return Err(SearchMainError::QueryTooLong);
    }
    Ok(query)
}

#[cfg(test)]
mod tests {
    use super::{SearchMainError, normalize_query};

    #[test]
    fn normalizes_whitespace_without_changing_query_terms() {
        assert_eq!(
            normalize_query("  早晨   跑步 ").expect("query should normalize"),
            "早晨 跑步"
        );
    }

    #[test]
    fn rejects_empty_and_oversized_queries() {
        assert!(matches!(
            normalize_query("  ").expect_err("empty query should be rejected"),
            SearchMainError::EmptyQuery
        ));
        assert!(matches!(
            normalize_query(&"x".repeat(201)).expect_err("long query should be rejected"),
            SearchMainError::QueryTooLong
        ));
    }
}
