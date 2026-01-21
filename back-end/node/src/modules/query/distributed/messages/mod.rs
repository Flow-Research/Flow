//! Network message types for distributed search.
//!
//! Defines the DAG-CBOR encoded messages used for search query
//! broadcast and response handling over GossipSub.

pub mod query;
pub mod response;

pub use query::SearchQueryMessage;
pub use response::{NetworkSearchResult, SearchResponseMessage};
