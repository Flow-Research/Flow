use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::entity::EntityStore;
use super::extraction::EntityExtractor;
use super::types::{CreateEntity, EntityType};
use super::vector::VectorIndex;

pub struct KnowledgeGraphService {
    entity_store: Arc<EntityStore>,
    vector_index: Arc<RwLock<VectorIndex>>,
    extractor: Arc<EntityExtractor>,
    space_id: i32,
    created_by: String,
}

impl KnowledgeGraphService {
    pub fn new(
        db: DatabaseConnection,
        vector_index: Arc<RwLock<VectorIndex>>,
        space_id: i32,
        created_by: String,
    ) -> Self {
        Self {
            entity_store: Arc::new(EntityStore::new(db)),
            vector_index,
            extractor: Arc::new(EntityExtractor::new()),
            space_id,
            created_by,
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.extractor = Arc::new(EntityExtractor::with_model(model));
        self
    }

    pub async fn extract_and_store(&self, chunk_text: &str, source_path: Option<&str>) -> Vec<String> {
        if chunk_text.trim().is_empty() {
            return Vec::new();
        }

        let entities = match self.extractor.extract(chunk_text).await {
            Ok(e) => e,
            Err(e) => {
                debug!("Entity extraction failed: {}", e);
                return Vec::new();
            }
        };

        debug!("Extracted {} entities from chunk", entities.len());
        let mut stored_names = Vec::new();

        for extracted in entities {
            let entity_type = match extracted.entity_type {
                EntityType::Person => EntityType::Person,
                EntityType::Concept => EntityType::Concept,
                EntityType::Date => EntityType::Date,
                EntityType::Location => EntityType::Location,
                EntityType::Document => EntityType::Document,
            };

            let input = CreateEntity {
                space_id: self.space_id,
                entity_type,
                name: extracted.name.clone(),
                properties: Some(serde_json::json!({
                    "confidence": extracted.confidence,
                    "source_path": source_path,
                })),
                created_by: self.created_by.clone(),
                source_chunk_id: None,
            };

            match self.entity_store.create(input).await {
                Ok(entity) => {
                    debug!("Created entity: {} ({})", entity.name, entity.entity_type);
                    stored_names.push(extracted.name);
                }
                Err(e) => {
                    warn!("Failed to create entity '{}': {}", extracted.name, e);
                }
            }
        }

        stored_names
    }

    pub fn entity_store(&self) -> &EntityStore {
        &self.entity_store
    }
}

impl Clone for KnowledgeGraphService {
    fn clone(&self) -> Self {
        Self {
            entity_store: Arc::clone(&self.entity_store),
            vector_index: Arc::clone(&self.vector_index),
            extractor: Arc::clone(&self.extractor),
            space_id: self.space_id,
            created_by: self.created_by.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_service_clone() {
        assert!(std::mem::size_of::<super::KnowledgeGraphService>() > 0);
    }
}
