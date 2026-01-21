pub mod distributed;
pub mod federated_search;
pub mod rag;
pub mod search;

pub use distributed::{
    DistributedSearchError, DistributedSearchRequest, DistributedSearchResponse,
    DistributedSearchResult, DistributedSearchScope, SearchConfig, SearchRouter,
};
pub use federated_search::{
    FederatedResult, FederatedResults, FederatedSearch, FederatedSearchError, ResultSource,
    SearchScope,
};
pub use rag::QueryPipeline;
pub use search::{SearchResult, SemanticSearch};
