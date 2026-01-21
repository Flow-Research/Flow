//! Entity for tracking locally published content.
//!
//! Records all content published by this node, including CID, metadata,
//! and references to Qdrant embeddings for cross-reference.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "published_content")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// CID of the root content (unique identifier)
    #[sea_orm(unique)]
    pub cid: String,

    /// Original file path (if file-based publish)
    pub path: Option<String>,

    /// Total size in bytes
    pub size: i64,

    /// Number of blocks in DAG
    pub block_count: i32,

    /// MIME type of content
    pub content_type: Option<String>,

    /// Title/name for display
    pub title: Option<String>,

    /// Qdrant collection where indexed
    pub qdrant_collection: Option<String>,

    /// Qdrant point IDs (JSON array of UUIDs)
    #[sea_orm(column_type = "Json", nullable)]
    pub qdrant_point_ids: Option<Json>,

    /// When content was published
    pub published_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
