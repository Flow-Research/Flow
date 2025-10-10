use crate::space_watcher::SpaceEvent;
use entity::{
    file_metadata::{Entity as FileMetadata, Model as FileMetadataModel},
    knowledge_node::{Entity as KnowledgeNode, Model as KnowledgeNodeModel},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use errors::AppError;
use log::info;
use serde_json;
use chrono::{DateTime, Utc};
use std::path::Path;

#[derive(Clone)]
pub struct KnowledgeService {
    db: DatabaseConnection,
}

impl KnowledgeService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn process_file_event(
        &self,
        space_key: &str,
        event: &SpaceEvent,
    ) -> Result<(), AppError> {
        // Update file metadata
        self.update_file_metadata(space_key, event).await?;
        
        // Extract knowledge from file if it's a text file
        if self.is_text_file(&event.path) {
            self.extract_knowledge_from_file(space_key, event).await?;
        }
        
        Ok(())
    }

    async fn update_file_metadata(
        &self,
        space_key: &str,
        event: &SpaceEvent,
    ) -> Result<(), AppError> {
        let path = Path::new(&event.path);
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        let file_extension = path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_string());

        let is_directory = event.event_type == crate::space_watcher::SpaceEventType::Deleted
            || path.is_dir();

        let metadata = entity::file_metadata::ActiveModel {
            space_key: Set(space_key.to_string()),
            file_path: Set(event.path.to_string_lossy().to_string()),
            file_name: Set(file_name),
            file_extension: Set(file_extension),
            file_size: Set(event.size.map(|s| s as i64)),
            file_hash: Set(event.hash.clone()),
            mime_type: Set(self.guess_mime_type(&event.path)),
            is_directory: Set(is_directory),
            last_modified: Set(DateTime::<Utc>::from(event.timestamp).into()),
            created_at: Set(DateTime::<Utc>::from(event.timestamp).into()),
            content_preview: Set(self.extract_content_preview(&event.path).ok()),
            tags: Set(None), // TODO: Implement tag extraction
            ..Default::default()
        };

        metadata
            .insert(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("Updated file metadata for: {}", event.path.display());
        Ok(())
    }

    async fn extract_knowledge_from_file(
        &self,
        space_key: &str,
        event: &SpaceEvent,
    ) -> Result<(), AppError> {
        if event.event_type == crate::space_watcher::SpaceEventType::Deleted {
            // Remove knowledge nodes for deleted files
            self.remove_knowledge_for_file(space_key, &event.path).await?;
            return Ok(());
        }

        // For now, create a simple knowledge node for each file
        let node_id = format!("file_{}", event.path.to_string_lossy().replace('/', "_"));
        
        let knowledge_node = entity::knowledge_node::ActiveModel {
            node_id: Set(node_id.clone()),
            node_type: Set("file".to_string()),
            title: Set(Some(event.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string())),
            content: Set(self.extract_content_preview(&event.path).ok()),
            source_file: Set(Some(event.path.to_string_lossy().to_string())),
            source_space: Set(Some(space_key.to_string())),
            created_at: Set(DateTime::<Utc>::from(event.timestamp).into()),
            updated_at: Set(DateTime::<Utc>::from(event.timestamp).into()),
            confidence_score: Set(Some(80)), // Default confidence
            metadata: Set(Some(serde_json::to_string(&event).unwrap_or_default())),
            ..Default::default()
        };

        knowledge_node
            .insert(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("Created knowledge node for file: {}", event.path.display());
        Ok(())
    }

    async fn remove_knowledge_for_file(
        &self,
        space_key: &str,
        file_path: &std::path::Path,
    ) -> Result<(), AppError> {
        // Remove knowledge nodes associated with this file
        KnowledgeNode::delete_many()
            .filter(entity::knowledge_node::Column::SourceFile.eq(file_path.to_string_lossy().to_string()))
            .filter(entity::knowledge_node::Column::SourceSpace.eq(space_key))
            .exec(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;

        info!("Removed knowledge nodes for deleted file: {}", file_path.display());
        Ok(())
    }

    fn is_text_file(&self, path: &std::path::Path) -> bool {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            matches!(extension.to_lowercase().as_str(), 
                "txt" | "md" | "rst" | "py" | "rs" | "js" | "ts" | "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf")
        } else {
            false
        }
    }

    fn guess_mime_type(&self, path: &std::path::Path) -> Option<String> {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "txt" => Some("text/plain".to_string()),
                "md" => Some("text/markdown".to_string()),
                "json" => Some("application/json".to_string()),
                "py" => Some("text/x-python".to_string()),
                "rs" => Some("text/x-rust".to_string()),
                "js" => Some("text/javascript".to_string()),
                "ts" => Some("text/typescript".to_string()),
                _ => None,
            }
        } else {
            None
        }
    }

    fn extract_content_preview(&self, path: &std::path::Path) -> Result<String, AppError> {
        if !path.is_file() {
            return Ok("Directory".to_string());
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| AppError::IO(e))?;
        
        // Return first 500 characters as preview
        if content.len() > 500 {
            Ok(format!("{}...", &content[..500]))
        } else {
            Ok(content)
        }
    }

    pub async fn get_file_metadata_for_space(
        &self,
        space_key: &str,
    ) -> Result<Vec<FileMetadataModel>, AppError> {
        println!("🔍 [KnowledgeService] Querying file metadata for space: {}", space_key);
        let result = FileMetadata::find()
            .filter(entity::file_metadata::Column::SpaceKey.eq(space_key))
            .order_by_desc(entity::file_metadata::Column::LastModified)
            .all(&self.db)
            .await
            .map_err(|e| {
                println!("❌ [KnowledgeService] Database error: {}", e);
                AppError::Storage(Box::new(e))
            });
        println!("🔍 [KnowledgeService] File metadata for space: {:?}", result);
        result
    }

    pub async fn get_knowledge_nodes_for_space(
        &self,
        space_key: &str,
    ) -> Result<Vec<KnowledgeNodeModel>, AppError> {
        KnowledgeNode::find()
            .filter(entity::knowledge_node::Column::SourceSpace.eq(space_key))
            .order_by_desc(entity::knowledge_node::Column::UpdatedAt)
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        space_key: Option<&str>,
    ) -> Result<Vec<KnowledgeNodeModel>, AppError> {
        let mut query_builder = KnowledgeNode::find();
        
        if let Some(space) = space_key {
            query_builder = query_builder.filter(entity::knowledge_node::Column::SourceSpace.eq(space));
        }
        
        // Simple text search in title and content
        query_builder = query_builder.filter(
            entity::knowledge_node::Column::Title.contains(query)
                .or(entity::knowledge_node::Column::Content.contains(query))
        );
        
        query_builder
            .order_by_desc(entity::knowledge_node::Column::UpdatedAt)
            .all(&self.db)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))
    }
}
