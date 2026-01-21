//! Space service layer for API business logic.
//!
//! This service encapsulates business logic for space operations,
//! keeping REST handlers thin and focused on HTTP concerns.

use chrono::{DateTime, Utc};
use entity::{kg_edge, kg_entity, space, space_index_status};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, ModelTrait, QueryFilter, QueryOrder};
use serde_json::Value as JsonValue;
use tracing::info;

// ============================================================================
// Service Error Types
// ============================================================================

/// Errors that can occur in space service operations.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("Space not found: {0}")]
    NotFound(String),

    #[error("Entity not found: {0}")]
    EntityNotFound(String),
}

// ============================================================================
// Domain Types
// ============================================================================

/// Space record from service layer.
#[derive(Debug, Clone)]
pub struct Space {
    pub id: i32,
    pub key: String,
    pub name: Option<String>,
    pub location: String,
    pub created_at: DateTime<Utc>,
}

/// Space indexing status.
#[derive(Debug, Clone)]
pub struct SpaceStatus {
    pub indexing_in_progress: bool,
    pub last_indexed: Option<DateTime<Utc>>,
    pub files_indexed: i32,
    pub chunks_stored: i32,
    pub files_failed: i32,
    pub last_error: Option<String>,
}

impl Default for SpaceStatus {
    fn default() -> Self {
        Self {
            indexing_in_progress: false,
            last_indexed: None,
            files_indexed: 0,
            chunks_stored: 0,
            files_failed: 0,
            last_error: None,
        }
    }
}

/// Knowledge graph entity.
#[derive(Debug, Clone)]
pub struct Entity {
    pub id: i32,
    pub cid: String,
    pub name: String,
    pub entity_type: String,
    pub properties: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
}

/// Knowledge graph edge direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDirection {
    Outgoing,
    Incoming,
}

/// Knowledge graph edge.
#[derive(Debug, Clone)]
pub struct Edge {
    pub id: i32,
    pub edge_type: String,
    pub other_id: i32,
    pub direction: EdgeDirection,
}

/// Entity with its edges.
#[derive(Debug)]
pub struct EntityWithEdges {
    pub entity: Entity,
    pub edges: Vec<Edge>,
}

// ============================================================================
// Space Service
// ============================================================================

/// Service for space-related business logic.
pub struct SpaceService<'a> {
    db: &'a DatabaseConnection,
}

impl<'a> SpaceService<'a> {
    /// Create a new space service instance.
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    /// List all spaces.
    pub async fn list_all(&self) -> Result<Vec<Space>, ServiceError> {
        let spaces = space::Entity::find().all(self.db).await?;

        Ok(spaces
            .into_iter()
            .map(|s| Space {
                id: s.id,
                key: s.key,
                name: s.name,
                location: s.location,
                created_at: s.time_created.into(),
            })
            .collect())
    }

    /// Get a space by key.
    pub async fn get_by_key(&self, key: &str) -> Result<Space, ServiceError> {
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(key))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Space '{}'", key)))?;

        Ok(Space {
            id: space.id,
            key: space.key,
            name: space.name,
            location: space.location,
            created_at: space.time_created.into(),
        })
    }

    /// Delete a space by key.
    pub async fn delete_by_key(&self, key: &str) -> Result<(), ServiceError> {
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(key))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Space '{}'", key)))?;

        space.delete(self.db).await?;

        info!(space_key = %key, "Space deleted");

        Ok(())
    }

    /// Get indexing status for a space.
    pub async fn get_status(&self, key: &str) -> Result<SpaceStatus, ServiceError> {
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(key))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Space '{}'", key)))?;

        let status = space_index_status::Entity::find()
            .filter(space_index_status::Column::SpaceId.eq(space.id))
            .one(self.db)
            .await?;

        Ok(match status {
            Some(s) => SpaceStatus {
                indexing_in_progress: s.indexing_in_progress.unwrap_or(false),
                last_indexed: s.last_indexed.map(|t| t.into()),
                files_indexed: s.files_indexed.unwrap_or(0),
                chunks_stored: s.chunks_stored.unwrap_or(0),
                files_failed: s.files_failed.unwrap_or(0),
                last_error: s.last_error,
            },
            None => SpaceStatus::default(),
        })
    }

    /// List entities in a space.
    pub async fn list_entities(&self, key: &str) -> Result<Vec<Entity>, ServiceError> {
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(key))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Space '{}'", key)))?;

        let entities = kg_entity::Entity::find()
            .filter(kg_entity::Column::SpaceId.eq(space.id))
            .order_by_desc(kg_entity::Column::CreatedAt)
            .all(self.db)
            .await?;

        Ok(entities
            .into_iter()
            .map(|e| Entity {
                id: e.id,
                cid: e.cid,
                name: e.name,
                entity_type: e.entity_type,
                properties: e.properties,
                created_at: e.created_at.into(),
            })
            .collect())
    }

    /// Get an entity by ID within a space, including its edges.
    pub async fn get_entity(
        &self,
        space_key: &str,
        entity_id: i32,
    ) -> Result<EntityWithEdges, ServiceError> {
        let space = space::Entity::find()
            .filter(space::Column::Key.eq(space_key))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Space '{}'", space_key)))?;

        let entity = kg_entity::Entity::find_by_id(entity_id)
            .filter(kg_entity::Column::SpaceId.eq(space.id))
            .one(self.db)
            .await?
            .ok_or_else(|| ServiceError::EntityNotFound(format!("Entity {}", entity_id)))?;

        let outgoing_edges = kg_edge::Entity::find()
            .filter(kg_edge::Column::SourceId.eq(entity_id))
            .all(self.db)
            .await?;

        let incoming_edges = kg_edge::Entity::find()
            .filter(kg_edge::Column::TargetId.eq(entity_id))
            .all(self.db)
            .await?;

        let mut edges: Vec<Edge> = outgoing_edges
            .into_iter()
            .map(|e| Edge {
                id: e.id,
                edge_type: e.edge_type,
                other_id: e.target_id,
                direction: EdgeDirection::Outgoing,
            })
            .collect();

        edges.extend(incoming_edges.into_iter().map(|e| Edge {
            id: e.id,
            edge_type: e.edge_type,
            other_id: e.source_id,
            direction: EdgeDirection::Incoming,
        }));

        Ok(EntityWithEdges {
            entity: Entity {
                id: entity.id,
                cid: entity.cid,
                name: entity.name,
                entity_type: entity.entity_type,
                properties: entity.properties,
                created_at: entity.created_at.into(),
            },
            edges,
        })
    }
}
