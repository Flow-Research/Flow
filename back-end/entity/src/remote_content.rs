//! Entity for tracking discovered remote content.
//!
//! Records content announced by other nodes via GossipSub, including
//! publisher information, indexing status, and Qdrant references.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "remote_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// CID of the root content (unique identifier)
    #[sea_orm(unique)]
    pub cid: String,

    /// Peer ID of publisher
    pub publisher_peer_id: String,

    /// DID of publisher (optional)
    pub publisher_did: Option<String>,

    /// Content metadata from announcement (JSON)
    #[sea_orm(column_type = "Json", nullable)]
    pub metadata: Option<Json>,

    /// Content type from announcement
    pub content_type: Option<String>,

    /// Title from announcement
    pub title: Option<String>,

    /// Size in bytes from announcement
    pub size: i64,

    /// Block count from announcement
    pub block_count: i32,

    /// When discovered via GossipSub
    pub discovered_at: DateTimeWithTimeZone,

    /// Whether blocks are cached locally
    pub cached: bool,

    /// Whether indexed in Qdrant
    pub indexed: bool,

    /// Last indexing error (if failed)
    pub index_error: Option<String>,

    /// Qdrant collection (if indexed)
    pub qdrant_collection: Option<String>,

    /// Number of embeddings created
    pub embedding_count: Option<i32>,

    /// When last indexed (if indexed)
    pub indexed_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
