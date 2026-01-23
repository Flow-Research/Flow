use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Document,
    Person,
    Concept,
    Date,
    Location,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Document => "document",
            EntityType::Person => "person",
            EntityType::Concept => "concept",
            EntityType::Date => "date",
            EntityType::Location => "location",
        }
    }
}

impl std::str::FromStr for EntityType {
    type Err = KgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "document" => Ok(EntityType::Document),
            "person" => Ok(EntityType::Person),
            "concept" => Ok(EntityType::Concept),
            "date" => Ok(EntityType::Date),
            "location" => Ok(EntityType::Location),
            _ => Err(KgError::InvalidEntityType(s.to_string())),
        }
    }
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Mentions,
    CreatedBy,
    RelatedTo,
    PartOf,
    OccurredOn,
    LocatedIn,
}

impl EdgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::Mentions => "mentions",
            EdgeType::CreatedBy => "created_by",
            EdgeType::RelatedTo => "related_to",
            EdgeType::PartOf => "part_of",
            EdgeType::OccurredOn => "occurred_on",
            EdgeType::LocatedIn => "located_in",
        }
    }
}

impl std::str::FromStr for EdgeType {
    type Err = KgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mentions" => Ok(EdgeType::Mentions),
            "created_by" => Ok(EdgeType::CreatedBy),
            "related_to" => Ok(EdgeType::RelatedTo),
            "part_of" => Ok(EdgeType::PartOf),
            "occurred_on" => Ok(EdgeType::OccurredOn),
            "located_in" => Ok(EdgeType::LocatedIn),
            _ => Err(KgError::InvalidEdgeType(s.to_string())),
        }
    }
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Debug, Error)]
pub enum KgError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Entity not found: {0}")]
    EntityNotFound(i32),
    #[error("Edge not found: {0}")]
    EdgeNotFound(i32),
    #[error("Invalid entity type: {0}")]
    InvalidEntityType(String),
    #[error("Invalid edge type: {0}")]
    InvalidEdgeType(String),
    #[error("CID generation failed: {0}")]
    CidGeneration(String),
    #[error("Max traversal depth exceeded")]
    MaxDepthExceeded,
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntity {
    pub space_id: i32,
    pub entity_type: EntityType,
    pub name: String,
    pub properties: Option<serde_json::Value>,
    pub created_by: String,
    pub source_chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEdge {
    pub space_id: i32,
    pub edge_type: EdgeType,
    pub source_id: i32,
    pub target_id: i32,
    pub properties: Option<serde_json::Value>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityFilter {
    pub entity_type: Option<EntityType>,
    pub name_contains: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EdgeFilter {
    pub edge_type: Option<EdgeType>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalResult {
    pub entity_id: i32,
    pub entity_name: String,
    pub entity_type: String,
    pub depth: usize,
    pub path: Vec<i32>,
}
